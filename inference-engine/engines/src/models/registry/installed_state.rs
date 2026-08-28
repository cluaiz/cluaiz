//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Live Installed State & Slot Manager (.cluaiz/engine/config/model_registry.json)
//! ═══════════════════════════════════════════════════════════════════════

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use crate::models::types::entities::SlotType;
use crate::models::types::manifest::{ModelRegistry, ModelRegistryEntry, RegistryModelFile, RegistryModelMetadata};
use crate::models::taxonomy::rules::{ModelCapabilities, UniversalTaskRules};

pub struct InstalledStateRegistry;

impl InstalledStateRegistry {
    /// Resolves the absolute path to model_registry.json dynamically via EnvironmentManager
    pub fn get_registry_path() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current().model_registry_json_path()
    }

    /// Resolves the absolute path to fast binary cache model_registry.bin dynamically via EnvironmentManager
    pub fn get_registry_bin_path() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current().model_registry_bin_path()
    }

    /// Loads the active ModelRegistry (Fast Path: .bin binary cache; Fallback: .json with auto-cache sync)
    pub fn load() -> ModelRegistry {
        let bin_path = Self::get_registry_bin_path();
        if bin_path.exists() {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(reg) = bincode::deserialize::<ModelRegistry>(&bytes) {
                    return reg;
                }
            }
        }

        let path = Self::get_registry_path();
        if !path.exists() {
            return ModelRegistry::default();
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return ModelRegistry::default(),
        };

        let mut reg: ModelRegistry = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => return ModelRegistry::default(),
        };

        let mut to_remove = Vec::new();
        for (id, entry) in &reg.installed_models {
            let dir_path = Path::new(&entry.local_dir);
            if !dir_path.exists() {
                to_remove.push(id.clone());
                continue;
            }

            if let Some(primary) = entry.files.iter().find(|f| f.is_primary) {
                let primary_file_path = dir_path.join(&primary.name);
                if !primary_file_path.exists() {
                    to_remove.push(id.clone());
                }
            }
        }

        if !to_remove.is_empty() {
            for id in to_remove {
                reg.installed_models.remove(&id);
            }
            let _ = Self::save(&reg);
        } else {
            // Save fast binary cache for subsequent 0-ms reads
            let _ = Self::save_bin_cache(&reg);
        }

        reg
    }

    /// Persists the active ModelRegistry to disk (.json and .bin)
    pub fn save(reg: &ModelRegistry) -> Result<(), String> {
        let path = Self::get_registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content = serde_json::to_string_pretty(reg)
            .map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&path, content).map_err(|e| format!("Write error: {}", e))?;

        // Sync fast binary cache
        let _ = Self::save_bin_cache(reg);
        Ok(())
    }

    /// Saves fast binary cache for 0-ms instant deserialization
    pub fn save_bin_cache(reg: &ModelRegistry) -> Result<(), String> {
        let bin_path = Self::get_registry_bin_path();
        if let Some(parent) = bin_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = bincode::serialize(reg)
            .map_err(|e| format!("Binary serialization error: {}", e))?;
        std::fs::write(&bin_path, bytes).map_err(|e| format!("Binary write error: {}", e))?;
        Ok(())
    }

    /// Registers a newly installed model entry into model_registry.json
    pub fn register_model(entry: ModelRegistryEntry) -> Result<(), String> {
        let mut reg = Self::load();
        reg.installed_models.insert(entry.id.clone(), entry);
        Self::save(&reg)
    }

    /// Removes an uninstalled model from model_registry.json
    pub fn unregister_model(model_id: &str) -> Result<(), String> {
        let mut reg = Self::load();
        reg.installed_models.remove(model_id);
        Self::save(&reg)
    }

    /// Retrieves all installed models for a specific SlotType
    pub fn get_models_for_slot(slot: &SlotType) -> Vec<ModelRegistryEntry> {
        let reg = Self::load();
        let target_category = slot.as_str();
        reg.installed_models
            .into_values()
            .filter(|m| m.category == target_category)
            .collect()
    }

    /// Scans the 5 sovereign model directories and synchronizes model_registry.json
    pub fn sync_from_disk(models_root: &Path) -> ModelRegistry {
        let mut reg = Self::load();
        let mut changes_made = false;

        let categories = ["chat", "embedding", "ingest", "tts", "stt"];
        for cat in &categories {
            let cat_dir = models_root.join(cat);
            if !cat_dir.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(&cat_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(ft) = entry.file_type() {
                        if ft.is_dir() {
                            let id = entry.file_name().to_string_lossy().to_string();
                            let mut all_weight_files = Vec::new();
                            let mut extra_files_list = Vec::new();

                            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                                for f in sub_entries.filter_map(|e| e.ok()) {
                                    let fname = f.file_name().to_string_lossy().to_string();
                                    if !fname.starts_with('.') && fname != "cluaiz-engine.ready" {
                                        let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                                        let fpath = f.path();

                                        if fpath.is_dir() {
                                            let mut subfolder_files = Vec::new();
                                            if let Ok(deep_entries) = std::fs::read_dir(&fpath) {
                                                for df in deep_entries.filter_map(|e| e.ok()) {
                                                    let dfname = df.file_name().to_string_lossy().to_string();
                                                    subfolder_files.push(serde_json::Value::String(dfname));
                                                }
                                            }
                                            let mut sub_map = serde_json::Map::new();
                                            sub_map.insert(fname, serde_json::Value::Array(subfolder_files));
                                            extra_files_list.push(serde_json::Value::Object(sub_map));
                                        } else if fname.ends_with(".gguf") || fname.ends_with(".onnx") {
                                            all_weight_files.push((fpath, fname, size));
                                        } else {
                                            extra_files_list.push(serde_json::Value::String(fname));
                                        }
                                    }
                                }
                            }
                            let extra_files = serde_json::Value::Array(extra_files_list);

                            if all_weight_files.is_empty() {
                                continue;
                            }

                            let (p_path, p_name, _) = &all_weight_files[0];
                            let p_path_clone = p_path.clone();
                            let p_name_clone = p_name.clone();

                            let format_type = if p_name_clone.ends_with(".gguf") {
                                "gguf"
                            } else {
                                "onnx"
                            };

                            let (slot_type, final_caps, mut metadata, requires_gpu) =
                                crate::models::prober::ModelProber::discover(
                                    &p_path_clone,
                                    &entry.path(),
                                    cat,
                                );

                            let mut files = Vec::new();
                            for (_fpath, fname, fsize) in &all_weight_files {
                                files.push(RegistryModelFile {
                                    name: fname.clone(),
                                    size_bytes: *fsize,
                                    is_primary: fname == &p_name_clone,
                                });
                            }

                            let hf_repo = reg
                                .installed_models
                                .get(&id)
                                .map(|e| e.huggingface_repo.clone())
                                .unwrap_or_default();

                            let registry_entry = ModelRegistryEntry {
                                id: id.clone(),
                                category: slot_type.as_str().to_string(),
                                format_type: format_type.to_string(),
                                huggingface_repo: hf_repo,
                                local_dir: entry.path().to_string_lossy().to_string(),
                                files,
                                extra_files,
                                supported_tasks: slot_type.supported_tasks(&final_caps),
                                requires_gpu,
                                metadata,
                            };

                            reg.installed_models.insert(id, registry_entry);
                            changes_made = true;
                        }
                    }
                }
            }
        }

        if changes_made {
            let _ = Self::save(&reg);
        }

        reg
    }
}
