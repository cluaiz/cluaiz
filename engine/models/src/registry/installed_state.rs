//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Live Installed State & Slot Manager (.cluaiz/engine/config/model_registry.json)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::types::entities::{SlotType, SlotTypeTasksExt};
use crate::models::types::manifest::{ModelRegistry, ModelRegistryEntry, RegistryModelFile};
use std::path::{Path, PathBuf};
use tracing::info;

// Versioned compact JSON avoids positional binary layout and serde_json::Value
// deserialize_any incompatibility. The human-readable registry remains authoritative.
const REGISTRY_CACHE_HEADER: &[u8] = b"CLREG003\n";

pub struct InstalledStateRegistry;

impl InstalledStateRegistry {
    /// Resolves the absolute path to model_registry.json dynamically via EnvironmentManager
    pub fn get_registry_path() -> PathBuf {
        cluaiz_core::EnvironmentManager::current().model_registry_json_path()
    }

    /// Resolves the versioned snapshot cache path through EnvironmentManager.
    pub fn get_registry_bin_path() -> PathBuf {
        cluaiz_core::EnvironmentManager::current().model_registry_bin_path()
    }

    /// Loads a compatible snapshot or migrates from authoritative JSON.
    pub fn load() -> ModelRegistry {
        let (mut reg, rebuild_cache) = Self::read_snapshot();
        let removed = Self::drop_missing(&mut reg);
        if Self::refresh_capabilities(&mut reg) || removed {
            let _ = Self::save(&reg);
        } else if rebuild_cache {
            let _ = Self::save_bin_cache(&reg);
        }
        reg
    }

    /// Resolve one installed model without refreshing/writing every other package.
    /// The caller inspects the selected package; the authoritative registry is reread each call.
    pub fn find(model_id: &str) -> Option<ModelRegistryEntry> {
        let (reg, _) = Self::read_snapshot();
        Self::find_in(&reg, model_id).cloned()
    }

    fn find_in<'a>(reg: &'a ModelRegistry, model_id: &str) -> Option<&'a ModelRegistryEntry> {
        let clean = model_id.trim_end_matches(".gguf");
        reg.installed_models
            .get(model_id)
            .or_else(|| reg.installed_models.get(clean))
            .or_else(|| {
                reg.installed_models.values().find(|entry| {
                    entry.id.eq_ignore_ascii_case(model_id) || entry.id.eq_ignore_ascii_case(clean)
                })
            })
            .filter(|entry| entry.primary_path().is_some_and(|path| path.is_file()))
    }

    fn read_snapshot() -> (ModelRegistry, bool) {
        Self::read_snapshot_at(&Self::get_registry_path(), &Self::get_registry_bin_path())
    }

    fn read_snapshot_at(path: &Path, bin_path: &Path) -> (ModelRegistry, bool) {
        // The .json is the source; the .bin is only a cache of it, used while it is not older than the .json
        let modified = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
        let bin_is_fresh = match (modified(bin_path), modified(path)) {
            (Some(bin), Some(json)) => bin >= json,
            (Some(_), None) => true,
            _ => false,
        };
        if bin_is_fresh {
            if let Ok(bytes) = std::fs::read(bin_path) {
                if let Ok(reg) = Self::decode_cache(&bytes) {
                    return (reg, false);
                }
            }
        }

        if !path.exists() {
            return (ModelRegistry::default(), false);
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return (ModelRegistry::default(), false),
        };

        let reg: ModelRegistry = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => return (ModelRegistry::default(), false),
        };
        (reg, true)
    }

    /// Removes entries whose folder or primary weight file is gone (deleted outside Cluaiz).
    /// Returns true when something was removed.
    fn drop_missing(reg: &mut ModelRegistry) -> bool {
        let before = reg.installed_models.len();
        reg.installed_models.retain(|_, entry| {
            let dir = Path::new(&entry.local_dir);
            dir.exists()
                && entry
                    .files
                    .iter()
                    .find(|f| f.is_primary)
                    .map(|p| dir.join(&p.name).exists())
                    .unwrap_or(true)
        });
        reg.installed_models.len() != before
    }

    /// Persists the active ModelRegistry to disk (.json and .bin)
    pub fn save(reg: &ModelRegistry) -> Result<(), String> {
        let path = Self::get_registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content =
            serde_json::to_string_pretty(reg).map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&path, content).map_err(|e| format!("Write error: {}", e))?;

        // Sync fast binary cache
        let _ = Self::save_bin_cache(reg);
        Ok(())
    }

    /// Saves a schema-versioned compact snapshot; no read-speed claim is made.
    pub fn save_bin_cache(reg: &ModelRegistry) -> Result<(), String> {
        let bin_path = Self::get_registry_bin_path();
        if let Some(parent) = bin_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = Self::encode_cache(reg)?;
        std::fs::write(&bin_path, bytes).map_err(|e| format!("Binary write error: {}", e))?;
        Ok(())
    }

    fn encode_cache(reg: &ModelRegistry) -> Result<Vec<u8>, String> {
        let mut bytes = REGISTRY_CACHE_HEADER.to_vec();
        bytes.extend(serde_json::to_vec(reg).map_err(|error| error.to_string())?);
        Ok(bytes)
    }

    fn decode_cache(bytes: &[u8]) -> Result<ModelRegistry, String> {
        let payload = bytes
            .strip_prefix(REGISTRY_CACHE_HEADER)
            .ok_or_else(|| "Incompatible registry cache version".to_string())?;
        serde_json::from_slice(payload).map_err(|error| error.to_string())
    }

    fn refresh_capabilities(reg: &mut ModelRegistry) -> bool {
        let mut changed = false;
        for entry in reg
            .installed_models
            .values_mut()
            .filter(|entry| entry.format_type == "gguf" || entry.format_type == "onnx")
        {
            let Some(primary) = entry.primary_path() else {
                continue;
            };
            let dir = Path::new(&entry.local_dir);
            let snapshot = crate::capabilities::PackageCapabilities::asset_snapshot(dir);
            let canonical = primary.canonicalize().ok();
            let contract = if entry.format_type == "onnx" {
                cluaiz_core::model_assets::PackageAssets::from_directory(dir).ok().flatten()
            } else { None };
            if entry.metadata.capabilities.as_ref().is_some_and(|package| {
                package.asset_snapshot == snapshot
                    && canonical.as_ref() == Some(&package.primary_file)
                    && package.asset_contract == contract
            }) {
                continue;
            }
            let package = crate::capabilities::PackageCapabilities::inspect(&primary).ok();
            let tasks = match entry.format_type.as_str() {
                "onnx" => package.as_ref().map(|p| p.onnx_tasks()),
                _ => package.as_ref().map(|p| p.gguf_tasks()),
            };
            for task in tasks.unwrap_or_default() {
                if !entry.supported_tasks.contains(&task) {
                    entry.supported_tasks.push(task);
                }
            }
            if entry.category == "chat" {
                let inputs = package
                    .as_ref()
                    .map(|package| package.supported_inputs())
                    .unwrap_or_default();
                entry
                    .supported_tasks
                    .retain(|task| task != "multimodal-vision" && task != "multimodal-audio");
                for (input, task) in [
                    (cluaiz_core::Modality::Image, "multimodal-vision"),
                    (cluaiz_core::Modality::Audio, "multimodal-audio"),
                ] {
                    if inputs.contains(&input) {
                        entry.supported_tasks.push(task.to_owned());
                    }
                }
            }
            entry.metadata.capabilities = package;
            changed = true;
        }
        changed
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
    pub fn entry_for_folder(
        category: &str,
        dir: &Path,
        primary: Option<&str>,
        huggingface_repo: &str,
    ) -> Option<ModelRegistryEntry> {
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
                            .map(|df| {
                                serde_json::Value::String(
                                    df.file_name().to_string_lossy().to_string(),
                                )
                            })
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
            Path::new(name)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        };
        let wanted = primary.map(str::to_string).or_else(from_manifest);
        let (p_path, p_name, _) = wanted
            .and_then(|w| weight_files.iter().find(|(_, n, _)| *n == w))
            .or_else(|| {
                weight_files
                    .iter()
                    .find(|(_, name, _)| !crate::fetcher::AssetBundler::is_helper_gguf(name))
            })?
            .clone();

        let format_type = if p_name.ends_with(".gguf") {
            "gguf"
        } else {
            "onnx"
        };
        let (slot_type, caps, metadata, requires_gpu) =
            crate::models::prober::ModelProber::discover(&p_path, dir, category);
        let files = weight_files
            .iter()
            .map(|(_, name, size)| RegistryModelFile {
                name: name.clone(),
                size_bytes: *size,
                is_primary: *name == p_name,
            })
            .collect();

        let mut supported_tasks = slot_type.supported_tasks(&caps);
        let package_tasks = metadata.capabilities.as_ref().map(|p| {
            if format_type == "onnx" {
                p.onnx_tasks()
            } else {
                p.gguf_tasks()
            }
        });
        for task in package_tasks.unwrap_or_default() {
            if !supported_tasks.contains(&task) {
                supported_tasks.push(task);
            }
        }
        Some(ModelRegistryEntry {
            id,
            category: slot_type.as_str().to_string(),
            format_type: format_type.to_string(),
            huggingface_repo: huggingface_repo.to_string(),
            local_dir: dir.to_string_lossy().to_string(),
            files,
            extra_files: serde_json::Value::Array(extra_files_list),
            supported_tasks,
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
            let Ok(entries) = std::fs::read_dir(models_root.join(category)) else {
                continue;
            };
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
            info!(
                "📦 [Registry] Registered model folders found on disk: {}",
                added.join(", ")
            );
            let _ = Self::save(&reg);
        }
        reg
    }
}

#[cfg(test)]
#[path = "installed_state_tests.rs"]
mod tests;
