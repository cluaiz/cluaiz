use cluaiz_core::EnvironmentManager;
use cluaiz_security::PermissionSchema;
use std::path::PathBuf;

/// Permissive Pre-Flight Guidance: Log informational capabilities and allow execution to proceed to engine
pub fn log_capability_guidance(
    schema: &PermissionSchema,
    slot_name: &str,
    required_tasks: &[&str],
) {
    if let Some(config) = schema.active_slots.get(slot_name) {
        if !config
            .supported_tasks
            .iter()
            .any(|t| required_tasks.contains(&t.as_str()))
        {
            tracing::info!(
                "ℹ️ [Slot Capability Guidance] Executing request for model in slot '{}' (supported_tasks: {:?}, requested: {:?}). Passing to engine driver.",
                slot_name, config.supported_tasks, required_tasks
            );
        }
    }
}

/// Dynamically resolve the absolute PathBuf of the currently active model in a slot
pub fn resolve_model_path(schema: &PermissionSchema, slot_name: &str) -> Option<PathBuf> {
    let config = schema.active_slots.get(slot_name)?;
    resolve_model_by_id(config.model_id.as_ref()?)
}

/// Dynamically resolve the absolute PathBuf of a specific model by its ID
pub fn resolve_model_by_id(model_id: &str) -> Option<PathBuf> {
    // 1. Primary: the installed registry (exact id first; a partial id only when it names one model)
    let installed_reg = cluaiz_models::registry::InstalledStateRegistry::load();
    if let [entry] = installed_reg.find(model_id).as_slice() {
        if let Some(path) = entry.primary_path() {
            return Some(path);
        }
    }

    // 2. Secondary fallback: Search directory tree
    let normalized_id = model_id.replace(':', "-");
    find_in_model_folders(&normalized_id)
}

/// Weight file in `<models>/<category>/<folder>` under the local or global models root, any category.
fn find_in_model_folders(folder: &str) -> Option<PathBuf> {
    let env = EnvironmentManager::current();
    let roots = [env.local_dir.join("models"), env.global_dir.join("models")];
    for root in &roots {
        let Ok(categories) = std::fs::read_dir(root) else {
            continue;
        };
        for category in categories.flatten() {
            let model_dir = category.path().join(folder);
            let Ok(entries) = std::fs::read_dir(&model_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext == "gguf" || ext == "onnx" || ext == "safetensors" {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}
