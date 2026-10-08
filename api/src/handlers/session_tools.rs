use axum::{
    extract::Path,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use engines::tools::{SessionToolBinding, ToolsEngine};

/// Request payload to attach, update, or detach tools for a session
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionToolsUpdateRequest {
    #[serde(default)]
    pub tools: Vec<SessionToolBinding>,
    #[serde(default)]
    pub detach: Vec<String>,
}

/// Decrements turns for all active tools in a session and purges tools that have reached 0 turns
pub fn decrement_session_turns(session_id: &str) {
    ToolsEngine::decrement_session_turns(session_id);
}

/// Returns the list of currently active tool IDs for a session
pub fn get_active_tool_ids_for_session(session_id: &str) -> Vec<String> {
    ToolsEngine::get_active_tool_ids_for_session(session_id)
}

/// GET /v1/chat/{session_id}/tools
/// Returns the current active tools and remaining turns for a session
pub async fn get_session_tools(Path(session_id): Path<String>) -> impl IntoResponse {
    let tools = ToolsEngine::get_session_tools(&session_id);
    
    Json(json!({
        "session_id": session_id,
        "active_tools": tools,
        "total_active": tools.len()
    }))
}

/// POST /v1/chat/{session_id}/tools
/// Attaches, updates, or detaches tools for a session with exact turn limits
pub async fn update_session_tools(
    Path(session_id): Path<String>,
    Json(payload): Json<SessionToolsUpdateRequest>,
) -> impl IntoResponse {
    let updated = ToolsEngine::update_session_tools(&session_id, payload.tools, payload.detach);

    Json(json!({
        "session_id": session_id,
        "status": "success",
        "message": "Session tools updated successfully",
        "active_tools": updated
    }))
}

/// GET /v1/tools
/// Unified global discovery endpoint returning all installed Skills, Plugins, and MCP servers
pub async fn get_all_tools() -> impl IntoResponse {
    let all_tools = ToolsEngine::list_all_tools().unwrap_or_default();

    Json(json!({
        "total": all_tools.len(),
        "tools": all_tools
    }))
}

