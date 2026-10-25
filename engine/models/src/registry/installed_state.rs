//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Live Installed State & Slot Manager (.cluaiz/engine/config/model_registry.json)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::types::entities::{SlotType, SlotTypeTasksExt};
use crate::models::types::manifest::{ModelRegistry, ModelRegistryEntry, RegistryModelFile};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use tracing::info;

/// The registry as last parsed, with the file it came from and a hash of its bytes: a lookup
/// reads the JSON (cheap) and parses it again only when it changed. The JSON is the only copy
/// on disk.
static PARSED: LazyLock<Mutex<Option<(PathBuf, u64, ModelRegistry)>>> = LazyLock::new(|| Mutex::new(None));

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub struct InstalledStateRegistry;

impl InstalledStateRegistry {
    /// Resolves the absolute path to model_registry.json dynamically via EnvironmentManager
    pub fn get_registry_path() -> PathBuf {
        cluaiz_core::EnvironmentManager::current().model_registry_json_path()
    }

    /// Loads the registry from `model_registry.json` (parsed again only when it changed).
    pub fn load() -> ModelRegistry {
        let mut reg = Self::read_snapshot();
        let removed = Self::drop_missing(&mut reg);
        if Self::refresh_capabilities(&mut reg) || removed {
            let _ = Self::save(&reg);
        }
        reg
    }

    /// Registry id of an installed weight file, else its file name
    pub fn id_for_path(path: &Path) -> String {
        let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let target = canonical(path);
        Self::load()
            .installed_models
            .values()
            .find(|e| e.primary_path().map(|p| canonical(&p) == target).unwrap_or(false))
            .map(|e| e.id.clone())
            .unwrap_or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    }

    /// Resolve one installed model without refreshing/writing every other package.
    /// The caller inspects the selected package; the authoritative registry is reread each call.
    pub fn find(model_id: &str) -> Option<ModelRegistryEntry> {
        let reg = Self::read_snapshot();
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

    fn read_snapshot() -> ModelRegistry {
        Self::read_snapshot_at(&Self::get_registry_path())
    }

    fn read_snapshot_at(path: &Path) -> ModelRegistry {
        let Ok(bytes) = std::fs::read(path) else {
            return ModelRegistry::default();
        };
        let hash = content_hash(&bytes);
        {
            let parsed = PARSED.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((cached_path, cached_hash, reg)) = parsed.as_ref() {
                if cached_path == path && *cached_hash == hash {
                    return reg.clone();
                }
            }
        }
        let Ok(reg) = serde_json::from_slice::<ModelRegistry>(&bytes) else {
            return ModelRegistry::default();
        };
        *PARSED.lock().unwrap_or_else(|e| e.into_inner()) = Some((path.to_path_buf(), hash, reg.clone()));
        reg
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

    /// Persists the active ModelRegistry to `model_registry.json`
    pub fn save(reg: &ModelRegistry) -> Result<(), String> {
        let path = Self::get_registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content =
            serde_json::to_string_pretty(reg).map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&path, &content).map_err(|e| format!("Write error: {}", e))?;
        *PARSED.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((path, content_hash(content.as_bytes()), reg.clone()));
        Ok(())
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
