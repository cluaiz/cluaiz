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
    /// Loaded by the model with `load_tool`: the tool stays in the conversation where it was
    /// loaded (not in the system prompt)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inline: bool,
    /// Kept until the user unpins it: never unloaded for being idle
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// Answers in a row that did not use it (a loaded tool goes after `idle_limit` of them)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub idle: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// Answers without use after which a loaded tool is unloaded (when its manifest sets no
/// `default_turns` above 0)
pub const IDLE_TURNS: u32 = 3;

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
                existing.pinned = new_tool.pinned;
                existing.idle = 0;
            } else {
                entry.push(new_tool);
            }
        }

        entry.clone()
    }

    /// After an answer: tools loaded with `load_tool` that it used start counting again, the others
    /// count one more idle answer, and those idle for `limit(id)` answers are unloaded (not
    /// pinned ones). Returns the unloaded ids.
    pub fn end_turn(session_id: &str, used: &[String], limit: impl Fn(&str) -> u32) -> Vec<String> {
        let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
        let Some(bindings) = registry.get_mut(session_id) else { return Vec::new() };
        let mut unloaded = Vec::new();
        bindings.retain_mut(|binding| {
            if !binding.inline || binding.pinned {
                return true;
            }
            if used.iter().any(|id| id.eq_ignore_ascii_case(&binding.id)) {
                binding.idle = 0;
                return true;
            }
            binding.idle += 1;
            if binding.idle >= limit(&binding.id).max(1) {
                unloaded.push(binding.id.clone());
                return false;
            }
            true
        });
        unloaded
    }

    /// Restores a chat's bindings saved on disk when RAM has none (after a restart)
    pub fn restore_if_missing(session_id: &str, saved: Vec<SessionToolBinding>) {
        let mut registry = SESSION_TOOL_REGISTRY.write().unwrap();
        if !registry.contains_key(session_id) && !saved.is_empty() {
            registry.insert(session_id.to_string(), saved);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(id: &str) -> SessionToolBinding {
        SessionToolBinding { id: id.into(), turns: -1, inline: true, pinned: false, idle: 0 }
    }

    #[test]
    fn test_idle_loaded_tools_go_pinned_and_bound_stay() {
        let chat = "idle-test";
        let pinned = SessionToolBinding { pinned: true, ..loaded("git") };
        let bound = SessionToolBinding { id: "fetch".into(), turns: -1, inline: false, pinned: false, idle: 0 };
        SessionToolManager::update_session_tools(chat, vec![loaded("filesystem"), loaded("pynotes"), pinned, bound], vec![]);
        let limit = |_: &str| 2;
        // Answer 1 uses filesystem: pynotes idles once
        assert!(SessionToolManager::end_turn(chat, &["filesystem".into()], limit).is_empty());
        // Answer 2 uses nothing: pynotes reaches 2 and goes; filesystem idles once
        assert_eq!(SessionToolManager::end_turn(chat, &[], limit), ["pynotes"]);
        // Answer 3: filesystem goes; the pinned and the bound tool stay
        assert_eq!(SessionToolManager::end_turn(chat, &[], limit), ["filesystem"]);
        let ids = SessionToolManager::get_active_tool_ids(chat);
        assert_eq!(ids, ["git", "fetch"]);
    }
}
