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
    /// Resolves the absolute path to model_registry.json
    pub fn get_registry_path() -> PathBuf {
        let primary = cluaiz_shared::environment::EnvironmentManager::current()
            .config_dir()
            .join("model_registry.json");
        if primary.exists() {
            return primary;
        }
        let fallback = PathBuf::from(".cluaiz/engine/config/model_registry.json");
        if fallback.exists() {
            return fallback;
        }
        primary
    }

    /// Loads the active model_registry.json from disk, pruning non-existent models
    pub fn load() -> ModelRegistry {
        let mut path = Self::get_registry_path();
        if !path.exists() {
            let fallback = PathBuf::from(".cluaiz/engine/config/model_registry.json");
            if fallback.exists() {
                path = fallback;
            } else {
                return ModelRegistry::default();
            }
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
        }

        reg
    }

    /// Persists the active ModelRegistry to disk
    pub fn save(reg: &ModelRegistry) -> Result<(), String> {
        let path = Self::get_registry_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let content = serde_json::to_string_pretty(reg)
            .map_err(|e| format!("Serialization error: {}", e))?;

        std::fs::write(&path, content).map_err(|e| format!("Write error: {}", e))?;
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
}
