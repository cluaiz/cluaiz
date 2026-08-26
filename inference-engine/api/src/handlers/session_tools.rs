use axum::{
    extract::Path,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use engines::neural_foundry::registry::registry_index::MasterRegistry;

/// A tool bound to a specific chat session with duration/turn tracking
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionToolBinding {
    /// Tool, Skill, or MCP identifier e.g. "cluaiz-math" or "code-review"
    pub id: String,
    /// Duration / Lifecycle:
    /// -1 = Permanent (All-time active in session)
    ///  0 = Single-turn ephemeral (auto-unloaded after 1 response)
    ///  N = N-turns countdown (decrements each turn, auto-unloaded at 0)
    pub turns: i32,
}

/// Request payload to attach, update, or detach tools for a session
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionToolsUpdateRequest {
    #[serde(default)]
    pub tools: Vec<SessionToolBinding>,
    #[serde(default)]
    pub detach: Vec<String>,
}

/// Thread-safe in-memory session tools registry (Keyed by session_id)
pub static SESSION_TOOL_REGISTRY: LazyLock<RwLock<HashMap<String, Vec<SessionToolBinding>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Decrements turns for all active tools in a session and purges tools that have reached 0 turns
pub fn decrement_session_turns(session_id: &str) {
    let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
    if let Some(bindings) = registry.get_mut(session_id) {
        let mut updated = Vec::new();
        for mut tool in bindings.drain(..) {
            if tool.turns == -1 {
                // Persistent: keep as is
                updated.push(tool);
            } else if tool.turns > 1 {
                // Decrement turn
                tool.turns -= 1;
                updated.push(tool);
            }
            // If tool.turns was 0 or 1, it has completed this turn and is dropped (auto-unloaded)
        }
        *bindings = updated;
    }
}

/// Returns the list of currently active tool IDs for a session
pub fn get_active_tool_ids_for_session(session_id: &str) -> Vec<String> {
    let registry = SESSION_TOOL_REGISTRY.read().unwrap();
    if let Some(bindings) = registry.get(session_id) {
        bindings.iter().map(|b| b.id.clone()).collect()
    } else {
        Vec::new()
    }
}

/// GET /v1/chat/{session_id}/tools
/// Returns the current active tools and remaining turns for a session
pub async fn get_session_tools(Path(session_id): Path<String>) -> impl IntoResponse {
    let registry = SESSION_TOOL_REGISTRY.read().unwrap();
    let tools = registry.get(&session_id).cloned().unwrap_or_default();
    
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
    let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
    let entry = registry.entry(session_id.clone()).or_insert_with(Vec::new);

    // 1. Process explicit detachments
    for detach_id in &payload.detach {
        entry.retain(|t| t.id != *detach_id);
    }

    // 2. Process updates / attachments
    for new_tool in payload.tools {
        if let Some(existing) = entry.iter_mut().find(|t| t.id == new_tool.id) {
            existing.turns = new_tool.turns;
        } else {
            entry.push(new_tool);
        }
    }

    Json(json!({
        "session_id": session_id,
        "status": "success",
        "message": "Session tools updated successfully",
        "active_tools": entry.clone()
    }))
}

/// GET /v1/tools
/// Unified global discovery endpoint returning all installed Skills, Plugins, and MCP servers
pub async fn get_all_tools() -> impl IntoResponse {
    let master = MasterRegistry::load().unwrap_or_default();
    let mut all_tools = Vec::new();

    // 1. Plugins
    for (id, p) in &master.plugins {
        all_tools.push(json!({
            "id": id,
            "name": p.name,
            "category": "plugin",
            "version": p.version,
            "description": p.description,
            "enabled": p.enabled,
            "execution_mode": p.execution_mode,
            "permissions": p.permissions,
        }));
    }

    // 2. MCP Servers
    for (id, m) in &master.mcp {
        all_tools.push(json!({
            "id": id,
            "name": m.name,
            "category": "mcp",
            "version": m.version,
            "description": m.description,
            "enabled": m.enabled,
            "execution_mode": m.execution_mode,
            "permissions": m.permissions,
        }));
    }

    // 3. Skills
    for (id, s) in &master.skills {
        all_tools.push(json!({
            "id": id,
            "name": s.name,
            "category": "skill",
            "version": s.version,
            "description": s.description,
            "enabled": s.enabled,
            "execution_mode": s.execution_mode,
            "permissions": s.permissions,
        }));
    }

    Json(json!({
        "total": all_tools.len(),
        "tools": all_tools
    }))
}
