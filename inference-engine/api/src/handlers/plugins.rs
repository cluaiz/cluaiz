use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

// ─── GET /v1/plugins/list ──────────────────────────────────────────────
pub async fn list_plugins(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_installed_components("plugin") {
        Ok(plugins) => Json(json!({"status": "success", "plugins": plugins})),
        Err(_) => Json(json!({"status": "success", "plugins": []}))
    }
}

#[derive(serde::Deserialize)]
pub struct InstallPluginPayload {
    pub plugin_name: String,
}

// ─── POST /v1/plugins/install ──────────────────────────────────────────
pub async fn install_plugin(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallPluginPayload>
) -> Json<Value> {
    let plugin_name = payload.plugin_name.clone();
    tokio::spawn(async move {
        let _ = engines::neural_foundry::registry::hub_installer::HubInstaller::install_component("plugin", &plugin_name).await;
    });

    Json(json!({"status": "success", "message": format!("Plugin '{}' installation queued natively in Engine.", payload.plugin_name)}))
}

// ─── DELETE /v1/plugins/remove ─────────────────────────────────────────
pub async fn remove_plugin(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallPluginPayload>
) -> Json<Value> {
    let plugin_name = payload.plugin_name.clone();
    match engines::neural_foundry::registry::hub_installer::HubInstaller::remove_component("plugin", &plugin_name).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("Plugin '{}' removed natively.", plugin_name)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to remove plugin: {}", e)}))
    }
}

// ─── GET /v1/plugins/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_component_cache("plugin") {
        Ok(report) => Json(json!({"status": "success", "message": report})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to list plugin cache: {}", e)}))
    }
}

// ─── DELETE /v1/plugins/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallPluginPayload>
) -> Json<Value> {
    let plugin_name = payload.plugin_name.clone();
    let target = if plugin_name == "all" || plugin_name.is_empty() { None } else { Some(plugin_name) };
    match engines::neural_foundry::registry::hub_installer::HubInstaller::clear_component_cache("plugin", target, true, true) {
        Ok(wiped) => Json(json!({"status": "success", "message": format!("Successfully wiped {} plugin caches.", wiped)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to clear plugin cache: {}", e)}))
    }
}
