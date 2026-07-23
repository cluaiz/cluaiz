use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use engines::neural_foundry::security::permission_schema::PermissionSchema;

fn get_local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };
    match socket.connect("8.8.8.8:80") {
        Ok(()) => match socket.local_addr() {
            Ok(addr) => Some(addr.ip().to_string()),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

// ─── GET /v1/system/permission ───────────────────────────────────────
pub async fn get_permission(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let schema = PermissionSchema::load();
    let models_root = cluaiz_shared::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());

    let registry = cluaiz_shared::utils::ModelRegistry::load();

    let mut available_chat_models: Vec<String> = Vec::new();
    let mut available_vector_models: Vec<String> = Vec::new();
    let mut available_vision_models: Vec<String> = Vec::new();
    let mut available_audio_models: Vec<String> = Vec::new();
    let mut all_models: Vec<String> = Vec::new();

    // Dynamically categorize from model_registry database
    for (id, entry) in &registry.installed_models {
        all_models.push(id.clone());
        match entry.category.as_str() {
            "chat" => available_chat_models.push(id.clone()),
            "embedding" => available_vector_models.push(id.clone()),
            "vision" => available_vision_models.push(id.clone()),
            "audio" => available_audio_models.push(id.clone()),
            _ => available_chat_models.push(id.clone()), // fallback
        }
    }

    // Check if active slots configuration points to models that no longer exist on disk (synced with registry state)
    let mut changed = false;
    let mut active_slots = schema.active_slots.clone();
    
    for (slot_name, slot_config) in active_slots.iter_mut() {
        if let Some(ref model_id) = slot_config.model_id {
            if !registry.installed_models.contains_key(model_id) {
                // Model was manually deleted or unregistered. Reset slot safely.
                slot_config.model_id = None;
                slot_config.format_type = None;
                slot_config.supported_tasks = Vec::new();
                changed = true;
            }
        }
    }

    let mut schema = schema;
    if changed {
        schema.active_slots = active_slots;
        let _ = schema.save(); // Persist clean state to Permission.json
    }

    let lan_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    
    // Inject available list properties into permission JSON so UI gets them
    let mut perm_json = serde_json::to_value(&schema).unwrap_or(json!({}));
    if let Some(obj) = perm_json.as_object_mut() {
        obj.insert("available_models".to_string(), json!(all_models));
        obj.insert("available_chat_models".to_string(), json!(available_chat_models));
        obj.insert("available_vector_models".to_string(), json!(available_vector_models));
        obj.insert("available_vision_models".to_string(), json!(available_vision_models));
        obj.insert("available_audio_models".to_string(), json!(available_audio_models));
    }

    Json(json!({
        "status": "success",
        "permission": perm_json,
        "lan_ip": lan_ip
    }))
}

// ─── POST /v1/system/permission ──────────────────────────────────────
pub async fn update_permission(
    State(_state): State<Arc<AppState>>,
    Json(mut payload): Json<PermissionSchema>,
) -> Json<Value> {
    payload.sync_active_slots();
    let _ = payload.save();
    Json(json!({
        "status": "success",
        "message": "permission.json successfully updated."
    }))
}
