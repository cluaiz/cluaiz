//! What the Engine added to each turn of a chat, so the next request renders the earlier turns
//! exactly as the model saw them.
//!
//! The App sends the conversation back as plain text: the user's messages without the tool
//! hints the Engine put in front of them, and the answers without the tool calls made on the
//! way. Rendering them that way would change the earlier turns, so the KV cache could not be
//! reused and the model would forget which tools it loaded and what they returned. The ledger
//! keeps, per user message, the hint block and the tool steps of that turn; the next request
//! puts them back in the same place. Loaded tools therefore stay where they were loaded instead
//! of moving into the system prompt.
//!
//! Kept in RAM and saved per chat to `<CLUAIZ_HOME>/memory/chats/<chat>/ledger.json`, together
//! with the chat's tool bindings, so a restart renders the same prompt.

use super::session_map::SessionToolBinding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// One tool step of a turn: the model's call and what it got back
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordedStep {
    /// `<tool_call>…</tool_call>` as the model wrote it
    pub call_xml: String,
    /// Function or tool name, used when the result is rendered as a `<tool_response>`
    pub name: String,
    pub result: String,
}

impl RecordedStep {
    /// The tool this step loaded (a `load_tool` call, or a call by the tool's id that loaded it)
    pub fn loaded_tool(&self) -> Option<String> {
        if !self.result.starts_with("Loaded tool") && !self.result.contains("It is loaded now") {
            return None;
        }
        loaded_tool_id(&self.call_xml)
    }
}

/// What the Engine added to one user message's turn
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TurnRecord {
    /// Identifies the user message: hash of its text and which occurrence of that text it is
    pub key: String,
    /// Text put in front of the user's message (the tool hint), empty if none
    #[serde(default)]
    pub block: String,
    #[serde(default)]
    pub steps: Vec<RecordedStep>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatLedger {
    #[serde(default)]
    pub turns: Vec<TurnRecord>,
    /// The chat's tool bindings when last saved (restored after a restart)
    #[serde(default)]
    pub bindings: Vec<SessionToolBinding>,
    /// Pinned notes put in this chat's system prompt (chosen when the chat starts, so the system
    /// prompt stays the same; notes pinned later travel with the message they arrive in)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Vec<String>>,
}

static LEDGERS: LazyLock<Mutex<HashMap<String, ChatLedger>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Key of a user message: its text and how many earlier user messages had the same text
pub fn turn_key(text: &str, occurrence: usize) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.trim().hash(&mut hasher);
    format!("{:016x}:{}", hasher.finish(), occurrence)
}

fn ledger_path(chat_id: &str) -> PathBuf {
    crate::memory::memory_root().join("chats").join(crate::memory::chat_stem(chat_id)).join("ledger.json")
}

impl ChatLedger {
    /// The chat's ledger (from RAM, else from disk, else empty)
    pub fn get(chat_id: &str) -> ChatLedger {
        let mut ledgers = LEDGERS.lock().unwrap_or_else(|e| e.into_inner());
        ledgers
            .entry(chat_id.to_string())
            .or_insert_with(|| {
                std::fs::read(ledger_path(chat_id))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default()
            })
            .clone()
    }

    /// Replaces the chat's ledger in RAM and on disk
    pub fn put(chat_id: &str, ledger: ChatLedger) {
        let path = ledger_path(chat_id);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_vec_pretty(&ledger) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&path, bytes) {
                    tracing::warn!("⚠️ [Ledger] Could not save {:?}: {}", path, e);
                }
            }
            Err(e) => tracing::warn!("⚠️ [Ledger] Could not encode the ledger of {}: {}", chat_id, e),
        }
        LEDGERS.lock().unwrap_or_else(|e| e.into_inner()).insert(chat_id.to_string(), ledger);
    }

    /// Forgets the chat entirely (RAM and disk)
    pub fn delete(chat_id: &str) {
        LEDGERS.lock().unwrap_or_else(|e| e.into_inner()).remove(chat_id);
        if let Some(dir) = ledger_path(chat_id).parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    pub fn record(&self, key: &str) -> Option<&TurnRecord> {
        self.turns.iter().find(|turn| turn.key == key)
    }

    /// Stores what the Engine added to a turn (replacing an earlier record of the same message)
    pub fn set_turn(&mut self, record: TurnRecord) {
        self.turns.retain(|turn| turn.key != record.key);
        if !record.block.is_empty() || !record.steps.is_empty() {
            self.turns.push(record);
        }
    }

    /// Replaces the recorded loads of `tool_id` with a short note (the model unloaded it).
    /// Returns true when something changed.
    pub fn tombstone(&mut self, tool_id: &str) -> bool {
        let mut changed = false;
        for step in self.turns.iter_mut().flat_map(|turn| turn.steps.iter_mut()) {
            if step.loaded_tool().is_some_and(|id| id.eq_ignore_ascii_case(tool_id.trim())) {
                step.result = format!("[Tool '{}' was unloaded.]", tool_id.trim());
                changed = true;
            }
        }
        changed
    }
}

/// The tool a recorded `load_tool` call (or a call by the tool's id) loaded
fn loaded_tool_id(call_xml: &str) -> Option<String> {
    let call: serde_json::Value = serde_json::from_str(
        call_xml.trim().trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim(),
    )
    .ok()?;
    let name = call.get("name").and_then(|n| n.as_str())?;
    if name == crate::tool_search::LOAD_TOOL_NAME {
        call.pointer("/arguments/id").and_then(|id| id.as_str()).map(|id| id.trim().to_string())
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(call: &str, result: &str) -> RecordedStep {
        RecordedStep { call_xml: format!("<tool_call>\n{}\n</tool_call>", call), name: "x".into(), result: result.into() }
    }

    #[test]
    fn test_turn_keys_tell_repeated_messages_apart() {
        assert_ne!(turn_key("ok", 0), turn_key("ok", 1));
        assert_eq!(turn_key(" ok ", 0), turn_key("ok", 0));
    }

    #[test]
    fn test_unloaded_tools_become_notes() {
        let mut ledger = ChatLedger::default();
        ledger.set_turn(TurnRecord {
            key: "a".into(),
            block: "<tools_hint>…</tools_hint>".into(),
            steps: vec![
                step(r#"{"name": "load_tool", "arguments": {"id": "filesystem"}}"#, "Loaded tool 'filesystem' (mcp)."),
                step(r#"{"name": "read_text_file", "arguments": {"path": "a.txt"}}"#, "hello"),
            ],
        });
        assert!(!ledger.tombstone("git"));
        assert!(ledger.tombstone("FileSystem"));
        assert_eq!(ledger.turns[0].steps[0].result, "[Tool 'FileSystem' was unloaded.]");
        assert_eq!(ledger.turns[0].steps[1].result, "hello");
        assert!(!ledger.tombstone("filesystem"));
    }

    #[test]
    fn test_empty_turns_are_not_kept() {
        let mut ledger = ChatLedger::default();
        ledger.set_turn(TurnRecord { key: "a".into(), ..Default::default() });
        assert!(ledger.turns.is_empty());
    }
}
