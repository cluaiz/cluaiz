use axum::{
    extract::Path,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use cluaiz_agents::{SessionToolBinding, ToolsEngine};

/// A tool to attach: `{"id", "turns"}`, or just its id to use the tool's default turns.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionToolInput {
    Binding(SessionToolBinding),
    Id(String),
}

impl SessionToolInput {
    fn into_binding(self) -> SessionToolBinding {
        match self {
            Self::Binding(binding) => binding,
            Self::Id(id) => {
                let turns = ToolsEngine::get_tool(&id).ok().flatten().map(|tool| tool.default_turns).unwrap_or(-1);
                SessionToolBinding { id, turns, inline: false, pinned: false, idle: 0 }
            }
        }
    }
}

/// Request payload to attach, update, or detach tools for a session
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionToolsUpdateRequest {
    #[serde(default)]
    pub tools: Vec<SessionToolInput>,
    #[serde(default)]
    pub detach: Vec<String>,
}

/// Request for `POST /v1/tools/discover`: the message to match, and optionally the user's
/// previous message (used for short follow-ups such as "phir se")
#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    pub query: String,
    #[serde(default)]
    pub previous: Option<String>,
    /// Also return the evidence for every tool with any match (for calibration)
    #[serde(default)]
    pub explain: bool,
}

/// POST /v1/tools/discover: which installed tools the Engine would suggest for a message, at
/// which level and why (a dry run of the matcher; nothing is loaded or started)
pub async fn discover_tools(Json(request): Json<DiscoverRequest>) -> impl IntoResponse {
    let catalog = cluaiz_agents::ToolSearch::catalog(&[]);
    let candidates = cluaiz_agents::ToolSearch::suggest(&catalog, &request.query, request.previous.as_deref());
    let mut response = json!({
        "query": request.query,
        "actionable": cluaiz_agents::tool_search::is_actionable(&request.query),
        "candidates": cluaiz_agents::ToolSearch::candidates_json(&catalog, &candidates),
        "installed": catalog.len()
    });
    if request.explain {
        response["evidence"] = cluaiz_agents::ToolSearch::evidence(&catalog, &request.query, request.previous.as_deref());
    }
    Json(response)
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
    let tools = payload.tools.into_iter().map(SessionToolInput::into_binding).collect();
    let updated = ToolsEngine::update_session_tools(&session_id, tools, payload.detach);

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

