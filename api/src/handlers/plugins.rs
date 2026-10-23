use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_agents::installer::ToolsInstaller;
use super::components::{clear_cache_response, install_component_response, list_cache_response, remove_component_response};

// ─── GET /v1/plugins/list ──────────────────────────────────────────────
pub async fn list_plugins(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match ToolsInstaller::list_installed_components("plugin") {
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
) -> (StatusCode, Json<Value>) {
    install_component_response("plugin", payload.plugin_name).await
}

// ─── DELETE /v1/plugins/remove ─────────────────────────────────────────
pub async fn remove_plugin(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallPluginPayload>
) -> (StatusCode, Json<Value>) {
    remove_component_response("plugin", payload.plugin_name).await
}

// ─── GET /v1/plugins/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    list_cache_response("plugin")
}

// ─── DELETE /v1/plugins/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallPluginPayload>
) -> (StatusCode, Json<Value>) {
    let name = payload.plugin_name;
    let target = (name != "all" && !name.is_empty()).then_some(name);
    clear_cache_response("plugin", target, true)
}

