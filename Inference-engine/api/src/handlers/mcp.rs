use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

pub async fn list_mcp(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"status": "success", "mcp_servers": []}))
}

#[derive(serde::Deserialize)]
pub struct InstallMcpPayload {
    pub mcp_name: String,
}

pub async fn install_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> Json<Value> {
    Json(json!({"status": "success", "message": format!("MCP server '{}' installation queued.", payload.mcp_name)}))
}

pub async fn remove_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> Json<Value> {
    let mcp_name = payload.mcp_name.clone();
    match engines::neural_foundry::registry::hub_installer::HubInstaller::remove_component("mcp", &mcp_name).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("MCP server '{}' removed natively.", mcp_name)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to remove MCP server: {}", e)}))
    }
}

// ─── GET /v1/mcp/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_component_cache("mcp") {
        Ok(report) => Json(json!({"status": "success", "message": report})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to list mcp cache: {}", e)}))
    }
}

// ─── DELETE /v1/mcp/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> Json<Value> {
    let mcp_name = payload.mcp_name.clone();
    let target = if mcp_name == "all" || mcp_name.is_empty() { None } else { Some(mcp_name) };
    match engines::neural_foundry::registry::hub_installer::HubInstaller::clear_component_cache("mcp", target, true, true) {
        Ok(wiped) => Json(json!({"status": "success", "message": format!("Successfully wiped {} MCP caches.", wiped)})),
        Err(e) => Json(json!({"status": "error", "message": format!("Failed to clear MCP cache: {}", e)}))
    }
}
