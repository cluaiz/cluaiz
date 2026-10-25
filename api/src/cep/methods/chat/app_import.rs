//! Read-only import of the App's existing chats into the Engine's chat event logs, so recall and
//! the Chat Map work for chats made before the event log existed. Opt-in (an API call), and it
//! never writes to the App's folders.
//!
//! The App keeps each chat in `<root>/chats/<chat id>/messages.jsonl` (one message per line:
//! `sender`, `text`, `date` in ms, `id`, optional `toolCalls` with `name`, `arguments`, `status`,
//! `result`) and `meta.json`. Roots: `%APPDATA%\cluaiz\data` and `<home>/.cluaiz/data`.

use cluaiz_agents::memory::transcript::message_key;
use cluaiz_agents::memory::{EventStore, JsonlStore, NewEvent};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The App's chat folders, in both roots, that exist
fn app_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(config) = dirs::config_dir() {
        roots.push(config.join("cluaiz").join("data").join("chats"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".cluaiz").join("data").join("chats"));
    }
    roots.into_iter().filter(|r| r.is_dir()).collect()
}

fn chat_folders(chat_id: Option<&str>) -> Vec<PathBuf> {
    let mut folders = Vec::new();
    for root in app_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.join("messages.jsonl").is_file() && chat_id.is_none_or(|id| id == name) {
                folders.push(path);
            }
        }
    }
    folders.sort();
    folders
}

fn rfc3339(ms: Option<i64>) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms?).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// The events of one App chat, in order
fn events_of(folder: &Path) -> (Vec<NewEvent>, usize, usize) {
    let mut events = Vec::new();
    let (mut messages, mut tools) = (0, 0);
    let text = std::fs::read_to_string(folder.join("messages.jsonl")).unwrap_or_default();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(message) = serde_json::from_str::<Value>(line) else { continue };
        let role = message.get("sender").and_then(Value::as_str).unwrap_or("user").to_lowercase();
        let body = message.get("text").and_then(Value::as_str).unwrap_or_default();
        let at = rfc3339(message.get("date").and_then(Value::as_i64));
        let id = message.get("id").and_then(Value::as_str);
        // Tool steps were made while answering: they come before the answer
        for call in message.get("toolCalls").and_then(Value::as_array).into_iter().flatten() {
            let name = call.get("name").and_then(Value::as_str).unwrap_or("tool");
            let args = call.get("arguments").map(cluaiz_agents::memory::records::redact);
            let files = args.as_ref().map(cluaiz_agents::memory::records::files_touched).unwrap_or_default();
            let mut call_event = NewEvent::new("tool_call", json!({ "tool": name, "args": args, "files": files, "source": "app-import" }));
            call_event.ts = at.clone();
            let mut result_event = NewEvent::new(
                "tool_result",
                json!({ "tool": name, "status": call.get("status"), "result": call.get("result").map(|r| r.as_str().map(str::to_string).unwrap_or_else(|| r.to_string())), "source": "app-import" }),
            );
            result_event.ts = at.clone();
            events.push(call_event);
            events.push(result_event);
            tools += 1;
        }
        let mut data = json!({ "role": role, "text": body, "pos": messages, "key": message_key(&role, body, id), "source": "app-import" });
        if let Some(id) = id {
            data["msg_id"] = json!(id);
        }
        let mut event = NewEvent::new("message", data);
        event.ts = at;
        events.push(event);
        messages += 1;
    }
    (events, messages, tools)
}

/// Imports App chats (read-only) into the Engine's chat event logs; `chat_id` = one chat, `None`
/// = all. A chat that already has a log is left as it is. Reports progress per chat.
pub fn import(chat_id: Option<&str>, dry_run: bool, task: &cluaiz_cep::TaskHandle) -> Value {
    let store = JsonlStore::shared();
    let folders = chat_folders(chat_id);
    let mut chats = Vec::new();
    for (done, folder) in folders.iter().enumerate() {
        if task.is_cancelled() {
            break;
        }
        let chat = folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let existing = store.read(&chat).map(|log| log.events.len()).unwrap_or(0);
        let (events, messages, tools) = events_of(folder);
        let status = if existing > 0 {
            "skipped: the chat already has an event log"
        } else if dry_run {
            "dry run"
        } else {
            match store.append(&chat, events.clone()).and_then(|_| store.sync(&chat)) {
                Ok(()) => "imported",
                Err(e) => {
                    tracing::warn!("⚠️ [Memory] Import of {} failed: {}", chat, e);
                    "failed"
                }
            }
        };
        let row = json!({ "chat_id": chat, "from": folder, "messages": messages, "tool_calls": tools, "events": events.len(), "status": status });
        task.event(row.clone());
        task.progress(cluaiz_cep::Progress { done: done as u64 + 1, total: Some(folders.len() as u64), unit: Some("chats".into()), message: None });
        chats.push(row);
    }
    json!({ "roots": app_roots(), "chats": chats })
}
