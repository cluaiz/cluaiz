//! Real read-only graph metadata and genuine same-repository helper completion.
use anyhow::{Context, Result};
use cluaiz_models::{
    capabilities::PackageCapabilities,
    fetcher::{HfVariant, HuggingFaceHub, ModelDownloader},
    types::manifest::ModelManifest,
};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

async fn complete_fixture_helpers(manifest: &ModelManifest, primary: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(
            std::env::var("CLUAIZ_FIXTURE_HTTP_TIMEOUT_MS")?.parse()?,
        ))
        .build()?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let result = ModelDownloader::complete_onnx_assets(
        manifest,
        primary,
        &client,
        tx,
        Arc::new(AtomicBool::new(false)),
    )
    .await;
    drain.await?;
    result.map_err(anyhow::Error::msg)
}
#[test]
#[ignore = "requires read-only ONNX fixture model paths"]
fn actual_onnx_packages_expose_graph_roles_without_loading_sessions() -> Result<()> {
    for (variable, task) in [
        ("CLUAIZ_ONNX_IMAGE_MODEL", "image-feature-extraction"),
        ("CLUAIZ_ONNX_TEXT_MODEL", "embedding"),
        ("CLUAIZ_ONNX_TTS_MODEL", "text-to-speech"),
        ("CLUAIZ_ONNX_STT_MODEL", "automatic-speech-recognition"),
    ] {
        let path = PathBuf::from(std::env::var(variable)?);
        let package = PackageCapabilities::inspect(&path)?;
        assert!(
            package.onnx_tasks().contains(&task.to_owned()),
            "{variable}: {:?}",
            package.onnx_tasks()
        );
        assert!(
            package.missing_onnx_assets().is_empty(),
            "{variable}: {:?}",
            package.missing_onnx_assets()
        );
        println!(
            "REAL ONNX PACKAGE {task}: {} graphs, inputs {:?}, helpers present",
            package.onnx_graphs.len(),
            package.supported_inputs()
        );
    }
    Ok(())
}
#[tokio::test]
#[ignore = "requires genuine fixture repository, source graph and network"]
async fn actual_same_repository_helper_completion_preserves_pooling_directory() -> Result<()> {
    let source = PathBuf::from(std::env::var("CLUAIZ_ONNX_TEXT_MODEL")?);
    let target = tempfile::tempdir()?;
    let primary = target.path().join("renamed.onnx");
    std::fs::hard_link(&source, &primary)?;
    let before = PackageCapabilities::inspect(&primary)?;
    assert!(before
        .missing_onnx_assets()
        .contains(&"tokenizer.json".into()));
    assert!(before
        .missing_onnx_assets()
        .contains(&"1_Pooling/config.json".into()));
    let remote_primary = std::env::var("CLUAIZ_FIXTURE_REMOTE_PRIMARY")?;
    let variant = HfVariant {
        variant_id: "isolated-fixture".into(),
        format_type: "onnx".into(),
        quant_tag: String::new(),
        primary_file: remote_primary.clone(),
        all_files: vec![remote_primary.clone()],
        filename: remote_primary,
        size_gb: 0.0,
    };
    let manifest = HuggingFaceHub::build_manifest(
        &std::env::var("CLUAIZ_FIXTURE_REPO")?,
        &variant,
        Some("feature-extraction"),
    );
    complete_fixture_helpers(&manifest, &primary).await?;
    let after = PackageCapabilities::inspect(&primary)?;
    assert!(after.missing_onnx_assets().is_empty());
    assert!(target.path().join("1_Pooling/config.json").is_file());
    assert!(!target.path().join("config.json").exists());
    let tokenizer: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.path().join("tokenizer.json"))?)?;
    assert!(tokenizer.get("model").is_some());
    let pool: serde_json::Value =
        serde_json::from_slice(&std::fs::read(target.path().join("1_Pooling/config.json"))?)?;
    assert_eq!(pool["pooling_mode_mean_tokens"], true);
    assert_eq!(
        std::fs::metadata(&source)?.len(),
        std::fs::metadata(&primary)?.len()
    );
    println!("REAL HELPER COMPLETION: same-repository tokenizer and pooling downloaded to correct paths; no session loaded");
    target.close().context("Fixture cleanup")?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires real local image export, explicitly selected source repository and fresh proof-output path"]
async fn actual_image_helper_completion_uses_selected_repository() -> Result<()> {
    let source = PathBuf::from(std::env::var("CLUAIZ_ONNX_IMAGE_MODEL")?);
    let target = tempfile::tempdir()?;
    let primary = target.path().join("local-export.onnx");
    std::fs::hard_link(&source, &primary)?;
    std::fs::hard_link(
        source
            .parent()
            .context("Model parent")?
            .join("tokenizer.json"),
        target.path().join("tokenizer.json"),
    )?;
    assert_eq!(
        PackageCapabilities::inspect(&primary)?.missing_onnx_assets(),
        ["preprocessor_config.json"]
    );
    // The ONNX is a supplied local export. The caller's selected repository is
    // used only for required helpers; no remote primary weight is requested.
    let variant = HfVariant {
        variant_id: "isolated-image-export".into(),
        format_type: "onnx".into(),
        quant_tag: String::new(),
        primary_file: "local-export.onnx".into(),
        all_files: vec![],
        filename: "local-export.onnx".into(),
        size_gb: 0.0,
    };
    let manifest = HuggingFaceHub::build_manifest(
        &std::env::var("CLUAIZ_FIXTURE_REPO")?,
        &variant,
        Some("image-feature-extraction"),
    );
    complete_fixture_helpers(&manifest, &primary).await?;
    assert!(PackageCapabilities::inspect(&primary)?
        .missing_onnx_assets()
        .is_empty());
    let bytes = std::fs::read(target.path().join("preprocessor_config.json"))?;
    let processor: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        processor["image_mean"]
            .as_array()
            .context("Image mean")?
            .len(),
        3
    );
    assert!(processor["do_normalize"]
        .as_bool()
        .context("Normalize flag")?);
    let output = PathBuf::from(std::env::var("CLUAIZ_HELPER_PROOF_OUTPUT")?);
    assert!(output.is_absolute());
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?
        .write_all(&bytes)?;
    println!("REAL IMAGE HELPER: selected repository -> existing completion owner -> genuine processor; no primary weight fetched");
    Ok(())
}
