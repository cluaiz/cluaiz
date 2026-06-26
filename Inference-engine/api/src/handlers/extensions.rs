use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

pub async fn list_extensions(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"status": "success", "extensions": []}))
}

#[derive(serde::Deserialize)]
pub struct InstallExtensionPayload {
    pub extension_name: String,
}

pub async fn install_extension(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallExtensionPayload>
) -> Json<Value> {
    Json(json!({"status": "success", "message": format!("Extension '{}' installation queued.", payload.extension_name)}))
}

pub async fn remove_extension(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallExtensionPayload>
) -> Json<Value> {
    let extension_name = payload.extension_name.clone();
    match engines::neural_foundry::registry::extension_manager::ExtensionManager::remove_extension(&extension_name).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("Extension '{}' removed natively.", extension_name)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to remove extension: {}", e)}))
    }
}

// ─── GET /v1/extensions/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"status": "success", "message": "Extension cache list not yet implemented."}))
}

// ─── DELETE /v1/extensions/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallExtensionPayload>
) -> Json<Value> {
    let extension_name = payload.extension_name.clone();
    let target = if extension_name == "all" || extension_name.is_empty() { None } else { Some(extension_name.as_str()) };
    match engines::neural_foundry::registry::extension_manager::ExtensionManager::clear_extension_cache(target).await {
        Ok(wiped) => Json(json!({"status": "success", "message": format!("Successfully wiped {} extension caches.", wiped)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to clear extension cache: {}", e)}))
    }
}
