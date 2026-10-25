use axum::response::{IntoResponse, Json};
use serde::Deserialize;
use serde_json::json;
use cluaiz_agents::ToolsEngine;

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

/// GET /v1/tools
/// Unified global discovery endpoint returning all installed Skills, Plugins, and MCP servers
pub async fn get_all_tools() -> impl IntoResponse {
    let all_tools = ToolsEngine::list_all_tools().unwrap_or_default();

    Json(json!({
        "total": all_tools.len(),
        "tools": all_tools
    }))
}

