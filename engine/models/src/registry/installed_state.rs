//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Live Installed State & Slot Manager (.cluaiz/engine/config/model_registry.json)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::{Path, PathBuf};
use tracing::info;
use crate::models::types::entities::{SlotType, SlotTypeTasksExt};
use crate::models::types::manifest::{ModelRegistry, ModelRegistryEntry, RegistryModelFile};

pub struct InstalledStateRegistry;

impl InstalledStateRegistry {
    /// Resolves the absolute path to model_registry.json dynamically via EnvironmentManager
    pub fn get_registry_path() -> PathBuf {
        cluaiz_core::EnvironmentManager::current().model_registry_json_path()
    }

    /// Resolves the absolute path to fast binary cache model_registry.bin dynamically via EnvironmentManager
    pub fn get_registry_bin_path() -> PathBuf {
        cluaiz_core::EnvironmentManager::current().model_registry_bin_path()
    }

    /// Loads the active ModelRegistry (Fast Path: .bin binary cache; Fallback: .json with auto-cache sync)
    pub fn load() -> ModelRegistry {
        let bin_path = Self::get_registry_bin_path();
        // The .json is the source; the .bin is only a cache of it, used while it is not older than the .json
        let modified = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
        let bin_is_fresh = match (modified(&bin_path), modified(&Self::get_registry_path())) {
            (Some(bin), Some(json)) => bin >= json,
            (Some(_), None) => true,
            _ => false,
        };
        if bin_is_fresh {
            if let Ok(bytes) = std::fs::read(&bin_path) {
                if let Ok(mut reg) = bincode::deserialize::<ModelRegistry>(&bytes) {
                    if Self::drop_missing(&mut reg) {
                        let _ = Self::save(&reg);
                    }
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

        if Self::drop_missing(&mut reg) {
            let _ = Self::save(&reg);
        } else {
            // Save fast binary cache for subsequent 0-ms reads
            let _ = Self::save_bin_cache(&reg);
        }

        reg
    }

    /// Removes entries whose folder or primary weight file is gone (deleted outside Cluaiz).
    /// Returns true when something was removed.
    fn drop_missing(reg: &mut ModelRegistry) -> bool {
        let before = reg.installed_models.len();
        reg.installed_models.retain(|_, entry| {
            let dir = Path::new(&entry.local_dir);
            dir.exists()
                && entry.files.iter().find(|f| f.is_primary).map(|p| dir.join(&p.name).exists()).unwrap_or(true)
        });
        reg.installed_models.len() != before
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

    /// Registry entry for one model folder. The primary weight file is `primary` when given, else the
    /// `huggingface_filename` of the folder's model_manifest.json (written at download), else the first
    /// .gguf / .onnx file by name. None when the folder holds no weight file.
    pub fn entry_for_folder(category: &str, dir: &Path, primary: Option<&str>, huggingface_repo: &str) -> Option<ModelRegistryEntry> {
        let id = dir.file_name()?.to_string_lossy().to_string();
        let mut weight_files = Vec::new();
        let mut extra_files_list = Vec::new();
        for f in std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
            let fname = f.file_name().to_string_lossy().to_string();
            if fname.starts_with('.') || fname == "cluaiz-engine.ready" {
                continue;
            }
            let fpath = f.path();
            if fpath.is_dir() {
                let subfolder_files: Vec<serde_json::Value> = std::fs::read_dir(&fpath)
                    .map(|deep| {
                        deep.filter_map(|e| e.ok())
                            .map(|df| serde_json::Value::String(df.file_name().to_string_lossy().to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let mut sub_map = serde_json::Map::new();
                sub_map.insert(fname, serde_json::Value::Array(subfolder_files));
                extra_files_list.push(serde_json::Value::Object(sub_map));
            } else if fname.ends_with(".gguf") || fname.ends_with(".onnx") {
                let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                weight_files.push((fpath, fname, size));
            } else {
                extra_files_list.push(serde_json::Value::String(fname));
            }
        }
        weight_files.sort_by(|a, b| a.1.cmp(&b.1));

        let from_manifest = || -> Option<String> {
            let text = std::fs::read_to_string(dir.join("model_manifest.json")).ok()?;
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            let name = value.get("huggingface_filename")?.as_str()?;
            Path::new(name).file_name().map(|n| n.to_string_lossy().to_string())
        };
        let wanted = primary.map(str::to_string).or_else(from_manifest);
        let (p_path, p_name, _) = wanted
            .and_then(|w| weight_files.iter().find(|(_, n, _)| *n == w))
            .or_else(|| weight_files.first())?
            .clone();

        let format_type = if p_name.ends_with(".gguf") { "gguf" } else { "onnx" };
        let (slot_type, caps, metadata, requires_gpu) = crate::models::prober::ModelProber::discover(&p_path, dir, category);
        let files = weight_files
            .iter()
            .map(|(_, name, size)| RegistryModelFile { name: name.clone(), size_bytes: *size, is_primary: *name == p_name })
            .collect();

        Some(ModelRegistryEntry {
            id,
            category: slot_type.as_str().to_string(),
            format_type: format_type.to_string(),
            huggingface_repo: huggingface_repo.to_string(),
            local_dir: dir.to_string_lossy().to_string(),
            files,
            extra_files: serde_json::Value::Array(extra_files_list),
            supported_tasks: slot_type.supported_tasks(&caps),
            requires_gpu,
            metadata,
        })
    }

    /// Registers every model folder in the category folders that is not in the registry yet (models
    /// copied in by hand, or downloaded while registration was missing). Registered models are left as
    /// they are, so a server start only probes new folders.
    pub fn sync_from_disk(models_root: &Path) -> ModelRegistry {
        let mut reg = Self::load();
        let mut added = Vec::new();
        for (category, _) in crate::models::registry::ModelVault::category_dirs() {
            let Ok(entries) = std::fs::read_dir(models_root.join(category)) else { continue };
            for entry in entries.filter_map(|e| e.ok()) {
                let dir = entry.path();
                let id = entry.file_name().to_string_lossy().to_string();
                if !dir.is_dir() || reg.installed_models.contains_key(&id) {
                    continue;
                }
                if let Some(registry_entry) = Self::entry_for_folder(category, &dir, None, "") {
                    added.push(id.clone());
                    reg.installed_models.insert(id, registry_entry);
                }
            }
        }
        if !added.is_empty() {
            info!("📦 [Registry] Registered model folders found on disk: {}", added.join(", "));
            let _ = Self::save(&reg);
        }
        reg
    }
}
