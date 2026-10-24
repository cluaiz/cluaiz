//! A chat's event log, written from chat requests (architecture §11.1). The matching rules live
//! in `cluaiz_agents::memory::transcript`; this module feeds them the request and the turn.
//!
//! **Client mode** (default): the client sends the whole conversation; new, edited and
//! retracted messages are logged. **Engine mode** (`history: "engine"`): the client sends only
//! the new messages and the Engine puts the logged transcript in front of them. After each
//! answer the Engine logs its tool steps, tool loads and unloads, a compaction, and the answer.
//! Private chats (`temporary_chat`) log nothing.

use super::types::{ExternalMessage, MessageContent};
use cluaiz_agents::memory::transcript::{self, ClientMessage};
use cluaiz_agents::memory::{records, NewEvent};
use cluaiz_agents::RecordedStep;
use serde_json::{json, Value};

/// What is logged with a new or edited message besides its text
fn extra_fields(message: &ExternalMessage, workspace: &str) -> serde_json::Map<String, Value> {
    let mut extra = serde_json::Map::new();
    // OpenAI tool fields are logged as sent (rendering them comes later)
    if let Some(calls) = &message.tool_calls {
        extra.insert("tool_calls".into(), calls.clone());
    }
    if let Some(call_id) = &message.tool_call_id {
        extra.insert("tool_call_id".into(), json!(call_id));
    }
    extra.insert("workspace".into(), json!(workspace));
    extra
}

/// Logs what a client request changed in the chat's transcript
pub fn ingest_request(chat: &str, messages: &[ExternalMessage]) -> Vec<cluaiz_agents::memory::Event> {
    let store = cluaiz_agents::memory::store_for(chat);
    let client: Vec<ClientMessage> = messages
        .iter()
        .map(|m| ClientMessage { role: m.role.clone(), text: m.content.text_content(), id: m.id.clone(), extra: Default::default() })
        .collect();
    // Only new and edited messages carry the extra fields, so they are made for those alone
    let workspace = crate::handlers::notes::current_workspace();
    let result = store.read_shared(chat).and_then(|log| {
        let mut events = transcript::diff_events(&log.events, &client);
        // The shared log is let go before the append, which then extends it in place
        drop(log);
        for event in &mut events {
            let pos = event.data.get("pos").and_then(Value::as_u64).map(|p| p as usize);
            if let (true, Some(message)) = (matches!(event.kind.as_str(), "message" | "message.edited"), pos.and_then(|p| messages.get(p))) {
                for (key, value) in extra_fields(message, &workspace) {
                    event.data.entry(key).or_insert(value);
                }
            }
        }
        store.append(chat, events)
    });
    result.unwrap_or_else(|e| {
        tracing::warn!("⚠️ [Memory] Could not log the request of chat {}: {}", chat, e);
        Vec::new()
    })
}

/// The logged transcript as messages, for engine mode
pub fn history_messages(chat: &str) -> Vec<ExternalMessage> {
    match cluaiz_agents::memory::store_for(chat).read_shared(chat) {
        Ok(log) => transcript::transcript(&log.events)
            .into_values()
            .map(|m| ExternalMessage { role: m.role, content: MessageContent::Text(m.text), ..Default::default() })
            .collect(),
        Err(e) => {
            tracing::warn!("⚠️ [Memory] Could not read the transcript of chat {}: {}", chat, e);
            Vec::new()
        }
    }
}

/// Logs what the Engine did in a turn (a compaction, tool loads and unloads, tool steps) and its
/// answer at position `answer_pos`, then makes the log durable
pub fn ingest_turn(chat: &str, steps: &[RecordedStep], tool_events: &[Value], compaction: Option<&Value>, answer: &str, answer_pos: usize) -> Vec<cluaiz_agents::memory::Event> {
    let mut events = Vec::new();
    if let Some(compaction) = compaction.filter(|c| !c.is_null()) {
        events.push(NewEvent::new("context_edit", json!({ "op": "compaction", "detail": compaction })));
    }
    for event in tool_events {
        if event.get("op").and_then(Value::as_str) == Some("approval") {
            events.push(NewEvent::new("approval", event.clone()));
        } else {
            events.push(NewEvent::new("context_edit", json!({ "op": event.get("op"), "target": event.get("target"), "ok": event.get("ok") })));
        }
    }
    for (index, step) in steps.iter().enumerate() {
        // Secrets in the arguments are taken out before the call is logged
        let call = records::redact_call(&step.call_xml);
        let files = records::call_json(&step.call_xml)
            .and_then(|c| c.get("arguments").cloned())
            .map(|args| records::files_touched(&args))
            .unwrap_or_default();
        events.push(NewEvent::new("tool_call", json!({ "step": index, "tool": step.name, "call": call, "files": files })));
        events.push(NewEvent::new("tool_result", json!({ "step": index, "tool": step.name, "result": step.result })));
    }
    events.push(transcript::answer_event(answer, answer_pos));
    let store = cluaiz_agents::memory::store_for(chat);
    match store.append(chat, events) {
        Ok(written) => {
            if let Err(e) = store.sync(chat) {
                tracing::warn!("⚠️ [Memory] Could not sync the log of chat {}: {}", chat, e);
            }
            written
        }
        Err(e) => {
            tracing::warn!("⚠️ [Memory] Could not log the turn of chat {}: {}", chat, e);
            Vec::new()
        }
    }
}
