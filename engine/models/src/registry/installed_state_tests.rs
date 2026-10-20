use super::*;
use crate::test_fixtures::{config, primary, projector};

#[test]
fn test_registry_old_capability_snapshot_gains_shared_contract_without_file_change() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fast_ar_int4.onnx");
    crate::test_fixtures::onnx(&path, "hidden_states", "logits", None);
    std::fs::write(
        dir.path().join("runtime_manifest.json"),
        serde_json::to_vec(&crate::test_fixtures::asset_manifest()).unwrap(),
    )
    .unwrap();
    let mut entry =
        InstalledStateRegistry::entry_for_folder("tts", dir.path(), Some("fast_ar_int4.onnx"), "")
            .unwrap();
    entry.metadata.capabilities.as_mut().unwrap().asset_contract = None;
    let id = entry.id.clone();
    let mut reg = ModelRegistry {
        installed_models: [(id.clone(), entry)].into_iter().collect(),
    };
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(reg.installed_models[&id]
        .metadata
        .capabilities
        .as_ref()
        .unwrap()
        .asset_contract
        .is_some());
    assert!(!InstalledStateRegistry::refresh_capabilities(&mut reg));
}

fn legacy_registry(dir: &Path) -> ModelRegistry {
    serde_json::from_value(serde_json::json!({"installed_models":{"opaque":{
        "id":"opaque","category":"chat","format_type":"gguf","huggingface_repo":"",
        "local_dir":dir,"files":[{"name":"main.gguf","size_bytes":1,"is_primary":true}],
        "extra_files":["config.json"],"supported_tasks":["chat-completion"],"requires_gpu":false,
        "metadata":{"architecture":"opaque","parameters":"unknown","context_window":"unknown",
            "quantization":null,"bit_depth":null,"chat_template":null}
    }}}))
    .unwrap()
}

#[test]
fn test_registry_legacy_json_and_versioned_cache_preserve_records() {
    let dir = tempfile::tempdir().unwrap();
    primary(dir.path());
    let reg = legacy_registry(dir.path());
    assert!(reg.installed_models["opaque"]
        .metadata
        .capabilities
        .is_none());
    // Existing positional cache cannot deserialize arbitrary serde_json::Value and
    // fields conditionally skipped by this schema. Reproduce before claiming migration.
    let old = bincode::serialize(&reg).unwrap();
    assert!(bincode::deserialize::<ModelRegistry>(&old).is_err());
    assert!(InstalledStateRegistry::decode_cache(&old).is_err());
    let bytes = InstalledStateRegistry::encode_cache(&reg).unwrap();
    let loaded = InstalledStateRegistry::decode_cache(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&reg).unwrap()
    );
    assert_eq!(loaded.installed_models["opaque"].files[0].name, "main.gguf");
    assert!(InstalledStateRegistry::decode_cache(b"CLREG999\n{}").is_err());
}

#[test]
fn test_registry_refresh_added_removed_projector_changed_config_and_primary() {
    let dir = tempfile::tempdir().unwrap();
    let path = primary(dir.path());
    let mut reg = legacy_registry(dir.path());
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(!InstalledStateRegistry::refresh_capabilities(&mut reg));
    config(dir.path());
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(reg.installed_models["opaque"]
        .supported_tasks
        .contains(&"multimodal-audio".into()));
    let helper = dir.path().join("renamed.gguf");
    projector(&helper, "32", true);
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(reg.installed_models["opaque"]
        .metadata
        .capabilities
        .as_ref()
        .unwrap()
        .projector
        .is_some());
    std::fs::remove_file(&helper).unwrap();
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(reg.installed_models["opaque"]
        .metadata
        .capabilities
        .as_ref()
        .unwrap()
        .projector
        .is_none());
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"vision_config":null,"audio_config":null}"#,
    )
    .unwrap();
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(!reg.installed_models["opaque"]
        .supported_tasks
        .contains(&"multimodal-audio".into()));
    std::fs::copy(&path, dir.path().join("alternate.gguf")).unwrap();
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    reg.installed_models.get_mut("opaque").unwrap().files[0].name = "alternate.gguf".into();
    assert!(InstalledStateRegistry::refresh_capabilities(&mut reg));
    assert!(reg.installed_models["opaque"]
        .metadata
        .capabilities
        .as_ref()
        .unwrap()
        .primary_file
        .ends_with("alternate.gguf"));
    let bytes = InstalledStateRegistry::encode_cache(&reg).unwrap();
    assert!(InstalledStateRegistry::decode_cache(&bytes)
        .unwrap()
        .installed_models["opaque"]
        .metadata
        .capabilities
        .is_some());
    assert!(path.is_file());
}

#[test]
fn test_registry_primary_selection_excludes_named_helper() {
    let dir = tempfile::tempdir().unwrap();
    primary(dir.path());
    projector(&dir.path().join("mmproj-F16.gguf"), "32", true);
    let entry = InstalledStateRegistry::entry_for_folder("chat", dir.path(), None, "").unwrap();
    assert_eq!(
        entry
            .files
            .iter()
            .find(|file| file.is_primary)
            .unwrap()
            .name,
        "main.gguf"
    );
}

#[test]
fn test_selected_lookup_reads_new_registry_without_refreshing_unrelated_packages() {
    let dir = tempfile::tempdir().unwrap();
    let selected = dir.path().join("selected");
    std::fs::create_dir(&selected).unwrap();
    let primary = primary(&selected);
    let mut reg = legacy_registry(&selected);
    let mut unrelated = reg.installed_models["opaque"].clone();
    unrelated.id = "unrelated".into();
    unrelated.local_dir = dir.path().join("absent").to_string_lossy().into();
    reg.installed_models.insert(unrelated.id.clone(), unrelated);
    let json_path = dir.path().join("registry.json");
    let bin_path = dir.path().join("registry.bin");
    let bytes = serde_json::to_vec(&reg).unwrap();
    std::fs::write(&json_path, &bytes).unwrap();
    let (snapshot, _) = InstalledStateRegistry::read_snapshot_at(&json_path, &bin_path);
    assert!(InstalledStateRegistry::find_in(&snapshot, "OPAQUE.gguf").is_some());
    assert_eq!(snapshot.installed_models.len(), 2);
    assert!(snapshot.installed_models["opaque"]
        .metadata
        .capabilities
        .is_none());
    assert_eq!(std::fs::read(&json_path).unwrap(), bytes);
    assert!(!bin_path.exists());
    reg.installed_models.get_mut("opaque").unwrap().id = "changed-id".into();
    std::fs::write(&json_path, serde_json::to_vec(&reg).unwrap()).unwrap();
    let (changed, _) = InstalledStateRegistry::read_snapshot_at(&json_path, &bin_path);
    assert!(InstalledStateRegistry::find_in(&changed, "changed-id").is_some());
    std::fs::remove_file(primary).unwrap();
    assert!(InstalledStateRegistry::find_in(&changed, "changed-id").is_none());
}
