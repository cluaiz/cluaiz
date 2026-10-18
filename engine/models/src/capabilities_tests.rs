use super::*;
use crate::test_fixtures::{config, gguf, primary, projector};
use crate::{
    prober::ModelProber,
    taxonomy::rules::ModelCapabilities,
    types::{entities::SlotTypeTasksExt, slot::SlotType},
};

#[test]
fn test_onnx_package_helpers_nested_refresh_and_category_preservation() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("neutral.onnx");
    crate::test_fixtures::onnx(
        &path,
        "pixel_values",
        "image_embeds",
        Some("weights/model.data"),
    );
    let package = PackageCapabilities::inspect(&path)?;
    assert_eq!(package.onnx_tasks(), ["image-feature-extraction"]);
    assert_eq!(package.supported_inputs(), [Modality::Image]);
    assert!(package.projector.is_none());
    assert_eq!(
        package.missing_onnx_assets(),
        ["preprocessor_config.json", "weights/model.data"]
    );
    std::fs::create_dir(dir.path().join("weights"))?;
    std::fs::write(dir.path().join("weights/model.data"), [0u8])?;
    std::fs::write(dir.path().join("preprocessor_config.json"), "{}")?;
    let complete = PackageCapabilities::inspect(&path)?;
    assert!(complete.missing_onnx_assets().is_empty());
    assert_ne!(package.asset_snapshot, complete.asset_snapshot);
    let entry = crate::registry::InstalledStateRegistry::entry_for_folder(
        "ingest",
        dir.path(),
        Some("neutral.onnx"),
        "",
    )
    .unwrap();
    assert_eq!(entry.category, "ingest");
    assert!(entry
        .supported_tasks
        .contains(&"image-feature-extraction".into()));
    assert!(entry.metadata.capabilities.is_some());
    Ok(())
}

#[test]
fn test_capabilities_attention_value_is_not_vision_and_missing_stays_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    let probe = GgufProber::probe_file(&path).unwrap();
    assert!(!probe.has_vision && !probe.has_audio);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(package.supported_inputs(), [Modality::Text]);
    assert_eq!(package.inputs[1].state, CapabilityState::Unknown);
    assert_eq!(package.inputs[2].state, CapabilityState::Unknown);
    assert!(package.projector.is_none());
}

#[test]
fn test_capabilities_renamed_package_preserves_audio_and_detects_actual_projector() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    config(dir.path());
    let helper = dir.path().join("arbitrary-name.gguf");
    projector(&helper, "32", true);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(
        package.supported_inputs(),
        [Modality::Text, Modality::Image, Modality::Audio]
    );
    let found = package.projector.unwrap();
    assert_eq!(found.path, std::fs::canonicalize(&helper).unwrap());
    assert_eq!(found.profile.audio_encoder, Some(true));
    assert_eq!(found.compatibility, CapabilityState::Unknown);
    let (slot, caps, _, _) = ModelProber::discover(&path, dir.path(), "chat");
    assert_eq!(slot.supported_inputs(&caps), ["text", "image", "audio"]);
    assert!(slot
        .supported_tasks(&caps)
        .contains(&"multimodal-audio".into()));
    assert!(!caps.is_asr && !caps.is_tts);
    std::fs::remove_file(&helper).unwrap();
    let missing = PackageCapabilities::inspect(&path).unwrap();
    assert!(missing.projector.is_none());
    assert!(missing.supported_inputs().contains(&Modality::Audio));
}

#[test]
fn test_capabilities_dimension_hints_remain_unverified_ambiguity_and_precision_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    config(dir.path());
    let first = dir.path().join("mmproj-F16.gguf");
    projector(&first, "64", true);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(
        package.projector.unwrap().compatibility,
        CapabilityState::Unknown
    );
    let second = dir.path().join("mmproj-BF16.gguf");
    projector(&second, "32", false);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert!(package.projector.is_none());
    assert_eq!(package.projector_candidates.len(), 2);
    assert!(package
        .diagnostics
        .iter()
        .any(|reason| reason.contains("explicit selection")));
    projector(&first, "32", false);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(
        package.projector.unwrap().path,
        std::fs::canonicalize(&first).unwrap()
    );
}

#[test]
fn test_capabilities_conflicting_malformed_and_unrelated_companion_reject_support() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    gguf(
        &path,
        &[("tokenizer.ggml.model", "vocabulary")],
        &[("clip.has_vision_encoder", false)],
        None,
    );
    config(dir.path());
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(package.inputs[1].state, CapabilityState::Unknown);
    assert!(!package.supported_inputs().contains(&Modality::Image));
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"vision_config":true,"audio_config":"invalid"}"#,
    )
    .unwrap();
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(package.inputs[1].state, CapabilityState::Unsupported);
    assert_eq!(package.inputs[2].state, CapabilityState::Unknown);
    std::fs::write(dir.path().join("config.json"), "malformed").unwrap();
    std::fs::write(dir.path().join("mmproj-broken.gguf"), "corrupt").unwrap();
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert!(package.projector.is_none());
    assert!(!package.diagnostics.is_empty());
    std::fs::write(&path, "corrupt").unwrap();
    assert!(PackageCapabilities::inspect(&path).is_err());
}

#[test]
fn test_capabilities_projector_alone_does_not_invent_model_inputs_or_stt_role() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    projector(&dir.path().join("helper.gguf"), "32", true);
    let package = PackageCapabilities::inspect(&path).unwrap();
    assert_eq!(package.supported_inputs(), [Modality::Text]);
    let caps = ModelCapabilities {
        has_audio: true,
        ..Default::default()
    };
    assert_eq!(SlotType::Chat.supported_inputs(&caps), ["text", "audio"]);
    assert_eq!(SlotType::Tts.supported_inputs(&caps), ["text"]);
    assert_eq!(SlotType::Stt.supported_inputs(&caps), ["audio"]);
    assert_eq!(SlotType::Chat.supported_outputs(), ["text"]);
    assert_eq!(SlotType::Tts.supported_outputs(), ["audio"]);
}

#[test]
#[ignore = "read-only audit requires CLUAIZ_CAPABILITY_AUDIT_ROOT"]
fn test_capabilities_installed_package_read_only() {
    let dir = PathBuf::from(std::env::var("CLUAIZ_CAPABILITY_AUDIT_ROOT").unwrap());
    let primary = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some("gguf")
                && !AssetBundler::is_helper_gguf(&path.to_string_lossy())
        })
        .unwrap();
    let package = PackageCapabilities::inspect(&primary).unwrap();
    println!("{}", serde_json::to_string_pretty(&package).unwrap());
    assert!(!package.inputs.is_empty());
}

#[test]
fn test_moonshine_raw_waveform_asr_package_detects_audio_and_asr_task() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let encoder_path = dir.path().join("encoder_model.onnx");
    let decoder_path = dir.path().join("decoder_model.onnx");
    crate::test_fixtures::onnx(&encoder_path, "input_values", "last_hidden_state", None);
    crate::test_fixtures::onnx(&decoder_path, "encoder_hidden_states", "logits", None);

    let package = PackageCapabilities::inspect(&encoder_path)?;
    assert_eq!(package.supported_inputs(), [Modality::Audio]);
    assert_eq!(package.onnx_tasks(), ["automatic-speech-recognition"]);
    assert_eq!(
        package.missing_onnx_assets(),
        ["preprocessor_config.json"]
    );

    std::fs::write(
        dir.path().join("preprocessor_config.json"),
        r#"{"feature_extractor_type":"MoonshineFeatureExtractor","sampling_rate":16000}"#,
    )?;
    let complete = PackageCapabilities::inspect(&encoder_path)?;
    assert!(complete.missing_onnx_assets().is_empty());
    Ok(())
}
