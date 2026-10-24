//! A chat's visible transcript from its events, and the events a client request adds to it.
//!
//! Client messages are matched to the logged transcript by position and key (the client's message
//! `id`, else a hash of role and text): a new one becomes `message`, a changed one
//! `message.edited` (superseding the old one), and logged messages the client no longer sends
//! `message.retracted`. Nothing is deleted.

use super::events::{Event, NewEvent};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

/// A message as the client sent it
#[derive(Debug, Clone, Default)]
pub struct ClientMessage {
    pub role: String,
    pub text: String,
    pub id: Option<String>,
    /// Extra fields logged as sent (e.g. OpenAI `tool_calls`, `tool_call_id`)
    pub extra: serde_json::Map<String, Value>,
}

/// A message of the logged transcript
#[derive(Debug, Clone, PartialEq)]
pub struct Logged {
    pub event_id: String,
    pub key: String,
    pub role: String,
    pub text: String,
}

/// Key of a message: its client id, else a hash of its role and text
pub fn message_key(role: &str, text: &str, id: Option<&str>) -> String {
    if let Some(id) = id.filter(|id| !id.is_empty()) {
        return format!("id:{id}");
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    role.to_lowercase().hash(&mut hasher);
    text.trim().hash(&mut hasher);
    format!("h:{:016x}", hasher.finish())
}

/// The visible transcript (position → message) from the chat's events
pub fn transcript(events: &[Event]) -> BTreeMap<u64, Logged> {
    let mut slots = BTreeMap::new();
    for event in events {
        let Some(pos) = event.data.get("pos").and_then(Value::as_u64) else { continue };
        match event.kind.as_str() {
            "message" | "message.edited" => {
                slots.insert(
                    pos,
                    Logged {
                        event_id: event.id.clone(),
                        key: event.str("key").unwrap_or_default().to_string(),
                        role: event.str("role").unwrap_or_default().to_string(),
                        text: event.str("text").unwrap_or_default().to_string(),
                    },
                );
            }
            "message.retracted" => {
                slots.remove(&pos);
            }
            _ => {}
        }
    }
    slots
}

fn message_event(kind: &str, pos: usize, message: &ClientMessage, extra: Value) -> NewEvent {
    let role = message.role.to_lowercase();
    let mut data = json!({
        "role": role,
        "text": message.text,
        "pos": pos,
        "key": message_key(&role, &message.text, message.id.as_deref()),
        "source": "client",
    });
    if let Some(id) = &message.id {
        data["msg_id"] = json!(id);
    }
    if let Value::Object(map) = &mut data {
        map.extend(message.extra.clone());
        if let Value::Object(extra) = extra {
            map.extend(extra);
        }
    }
    NewEvent::new(kind, data)
}

/// The events a client request adds to the logged transcript
pub fn diff(logged: &BTreeMap<u64, Logged>, messages: &[ClientMessage]) -> Vec<NewEvent> {
    let mut events = Vec::new();
    for (pos, message) in messages.iter().enumerate() {
        let role = message.role.to_lowercase();
        match logged.get(&(pos as u64)) {
            None => events.push(message_event("message", pos, message, json!({}))),
            // Same message (same key, or the Engine's own answer coming back as text) with the
            // same text: nothing new
            Some(old)
                if old.text.trim() == message.text.trim()
                    && (old.role == role || old.key == message_key(&role, &message.text, message.id.as_deref())) => {}
            Some(old) => events.push(message_event("message.edited", pos, message, json!({ "supersedes": old.event_id }))),
        }
    }
    for (pos, old) in logged.range(messages.len() as u64..) {
        events.push(NewEvent::new("message.retracted", json!({ "pos": pos, "retracts": old.event_id })));
    }
    events
}

/// The Engine's answer at `pos`
pub fn answer_event(text: &str, pos: usize) -> NewEvent {
    NewEvent::new(
        "message",
        json!({ "role": "assistant", "text": text, "pos": pos, "key": message_key("assistant", text, None), "source": "engine" }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, text: &str) -> ClientMessage {
        ClientMessage { role: role.into(), text: text.into(), ..Default::default() }
    }

    fn logged(events: &[NewEvent]) -> BTreeMap<u64, Logged> {
        let events: Vec<Event> = events
            .iter()
            .enumerate()
            .map(|(i, e)| Event { v: 1, id: format!("ev{i}"), chat: "c".into(), seq: i as u64 + 1, ts: String::new(), kind: e.kind.clone(), data: e.data.clone() })
            .collect();
        transcript(&events)
    }

    #[test]
    fn test_new_edited_and_retracted_messages() {
        let first = diff(&BTreeMap::new(), &[msg("user", "hi"), msg("assistant", "hello"), msg("user", "notes.txt padho")]);
        assert_eq!(first.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["message", "message", "message"]);
        let log = logged(&first);
        // Same history plus one: only the new message
        let next = diff(&log, &[msg("user", "hi"), msg("assistant", "hello"), msg("user", "notes.txt padho"), msg("assistant", "done")]);
        assert_eq!(next.len(), 1);
        // The user edited the last message
        let edit = diff(&log, &[msg("user", "hi"), msg("assistant", "hello"), msg("user", "README padho")]);
        assert_eq!((edit.len(), edit[0].kind.as_str()), (1, "message.edited"));
        assert_eq!(edit[0].data["supersedes"], "ev2");
        // Regenerate: the client sends less than was logged
        let retract = diff(&log, &[msg("user", "hi")]);
        assert_eq!(retract.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(), ["message.retracted", "message.retracted"]);
        let mut all = first.clone();
        all.extend(retract);
        assert_eq!(logged(&all).len(), 1);
    }

    #[test]
    fn test_the_engine_answer_coming_back_is_not_an_edit() {
        let mut events = diff(&BTreeMap::new(), &[msg("user", "hi")]);
        events.push(answer_event("Hello! ", 1));
        let log = logged(&events);
        assert!(diff(&log, &[msg("user", "hi"), msg("assistant", "Hello!")]).is_empty());
    }

    #[test]
    fn test_ids_take_priority_over_text() {
        let mut a = msg("user", "hi");
        a.id = Some("m1".into());
        let log = logged(&diff(&BTreeMap::new(), &[a.clone()]));
        let mut b = msg("user", "hi there");
        b.id = Some("m1".into());
        assert_eq!(diff(&log, &[b])[0].kind, "message.edited");
        assert!(diff(&log, &[a]).is_empty());
    }
}
