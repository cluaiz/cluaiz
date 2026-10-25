use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_agents::installer::ToolsInstaller;
use super::components::{clear_cache_response, install_component_response, list_cache_response, remove_component_response};

pub async fn list_mcp(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match ToolsInstaller::list_installed_components("mcp") {
        Ok(mcp_servers) => Json(json!({"status": "success", "mcp_servers": mcp_servers})),
        Err(_) => Json(json!({"status": "success", "mcp_servers": []}))
    }
}

#[derive(serde::Deserialize)]
pub struct InstallMcpPayload {
    pub mcp_name: String,
}

pub async fn install_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    install_component_response("mcp", payload.mcp_name).await
}

pub async fn remove_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    remove_component_response("mcp", payload.mcp_name).await
}

// ─── GET /v1/mcp/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    list_cache_response("mcp")
}

// ─── DELETE /v1/mcp/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    let name = payload.mcp_name;
    let target = (name != "all" && !name.is_empty()).then_some(name);
    clear_cache_response("mcp", target, true)
}
