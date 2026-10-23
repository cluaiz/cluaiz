use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};
use serde::{Deserialize, Serialize};

/// A tool binding for a specific chat session with duration/turn tracking
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SessionToolBinding {
    pub id: String,
    /// Duration / Lifecycle:
    /// -1 = Permanent (All-time active in session)
    ///  0 = Single-turn ephemeral (auto-unloaded after 1 response)
    ///  N = N-turns countdown (decrements each turn, auto-unloaded at 0)
    pub turns: i32,
}

pub static SESSION_TOOL_REGISTRY: LazyLock<RwLock<HashMap<String, Vec<SessionToolBinding>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Tools the model unloaded in a session, with the answers left before they may be suggested
/// again (session id -> tool id -> answers)
pub static SUPPRESSED_TOOLS: LazyLock<RwLock<HashMap<String, HashMap<String, u32>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub struct SessionToolManager;

impl SessionToolManager {
    /// Returns the active tools bound to a session
    pub fn get_session_tools(session_id: &str) -> Vec<SessionToolBinding> {
        let registry = SESSION_TOOL_REGISTRY.read().unwrap();
        registry.get(session_id).cloned().unwrap_or_default()
    }

    /// Returns active tool IDs for a session
    pub fn get_active_tool_ids(session_id: &str) -> Vec<String> {
        let registry = SESSION_TOOL_REGISTRY.read().unwrap();
        if let Some(bindings) = registry.get(session_id) {
            bindings.iter().map(|b| b.id.clone()).collect()
        } else {
            Vec::new()
        }
    }

    /// Attaches or updates tools for a session
    pub fn update_session_tools(session_id: &str, tools: Vec<SessionToolBinding>, detach: Vec<String>) -> Vec<SessionToolBinding> {
        let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
        let entry = registry.entry(session_id.to_string()).or_insert_with(Vec::new);

        for detach_id in &detach {
            entry.retain(|t| &t.id != detach_id);
        }

        for new_tool in tools {
            if let Some(existing) = entry.iter_mut().find(|t| t.id == new_tool.id) {
                existing.turns = new_tool.turns;
            } else {
                entry.push(new_tool);
            }
        }

        entry.clone()
    }

    /// Keeps a tool the model unloaded out of the suggestions for the next `answers` answers
    pub fn suppress(session_id: &str, tool_id: &str, answers: u32) {
        let mut suppressed = SUPPRESSED_TOOLS.write().unwrap();
        suppressed.entry(session_id.to_string()).or_default().insert(tool_id.to_string(), answers);
    }

    /// Tools that must not be suggested in this session right now
    pub fn suppressed_tool_ids(session_id: &str) -> Vec<String> {
        SUPPRESSED_TOOLS
            .read()
            .unwrap()
            .get(session_id)
            .map(|tools| tools.keys().cloned().collect())
            .unwrap_or_default()
    }
}
