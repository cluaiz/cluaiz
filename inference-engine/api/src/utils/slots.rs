use std::path::PathBuf;
use axum::{response::Response, http::StatusCode};
use serde_json::json;
use engines::neural_foundry::security::permission_schema::PermissionSchema;
use cluaiz_shared::environment::EnvironmentManager;

/// Permissive Pre-Flight Guidance: Log informational capabilities and allow execution to proceed to engine
pub fn require_capability(schema: &PermissionSchema, slot_name: &str, required_tasks: &[&str]) -> Result<(), Response> {
    if let Some(config) = schema.active_slots.get(slot_name) {
        if !config.supported_tasks.iter().any(|t| required_tasks.contains(&t.as_str())) {
            tracing::info!(
                "ℹ️ [Slot Capability Guidance] Executing request for model in slot '{}' (supported_tasks: {:?}, requested: {:?}). Passing to engine driver.",
                slot_name, config.supported_tasks, required_tasks
            );
        }
    }
    Ok(())
}

/// Dynamically resolve the absolute PathBuf of the currently active model in a slot
pub fn resolve_model_path(schema: &PermissionSchema, slot_name: &str) -> Option<PathBuf> {
    let config = schema.active_slots.get(slot_name)?;
    let model_id = config.model_id.as_ref()?.replace(':', "-");
    
    let env = EnvironmentManager::current();
    let roots = [env.local_dir.join("models"), env.global_dir.join("models")];
    let categories = ["chat", "embedding", "vision", "audio", "code"];
    
    for root in &roots {
        for category in &categories {
            let model_dir = root.join(category).join(&model_id);
            if model_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&model_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                            // Support multiple formats as per our MoE architecture
                            if ext == "gguf" || ext == "onnx" || ext == "safetensors" {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
    }
    
    None
}

/// Dynamically resolve the absolute PathBuf of a specific model by its ID
pub fn resolve_model_by_id(model_id: &str) -> Option<PathBuf> {
    let normalized_id = model_id.replace(':', "-");
    
    let env = EnvironmentManager::current();
    let roots = [env.local_dir.join("models"), env.global_dir.join("models")];
    let categories = ["chat", "embedding", "vision", "audio", "code"];
    
    for root in &roots {
        for category in &categories {
            let model_dir = root.join(category).join(&normalized_id);
            if model_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&model_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                            // Support multiple formats as per our MoE architecture
                            if ext == "gguf" || ext == "onnx" || ext == "safetensors" {
                                return Some(p);
                            }
                        }
                    }
                }
            }
        }
    }
    
    None
}
