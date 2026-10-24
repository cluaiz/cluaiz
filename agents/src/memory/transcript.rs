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

/// A logged message, borrowed: (event id, key, role, text)
type LoggedRef<'a> = (&'a str, &'a str, &'a str, &'a str);

/// The events a client request adds to the logged transcript
pub fn diff(logged: &BTreeMap<u64, Logged>, messages: &[ClientMessage]) -> Vec<NewEvent> {
    let mut slots: Vec<Option<LoggedRef>> = Vec::new();
    for (pos, m) in logged {
        let pos = *pos as usize;
        if slots.len() <= pos {
            slots.resize(pos + 1, None);
        }
        slots[pos] = Some((m.event_id.as_str(), m.key.as_str(), m.role.as_str(), m.text.as_str()));
    }
    let borrowed: Vec<SentRef> = messages.iter().map(|m| (m.role.as_str(), m.text.as_str(), m.id.as_deref())).collect();
    change_events(compare(&slots, &borrowed), |pos| messages[pos].clone())
}

/// A message the client sent, borrowed: (role, text, client id)
pub type SentRef<'a> = (&'a str, &'a str, Option<&'a str>);

/// What a client request changed at a position of the transcript
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    New(usize),
    /// The position and the event id of the message it replaces
    Edited(usize, String),
    /// The position and the event id of the message it removes
    Retracted(usize, String),
}

/// The changes a client request makes to the chat's events, from its messages borrowed: a long
/// chat sends every message with every request, so nothing is copied for the ones that match
pub fn changes(events: &[Event], messages: &[SentRef]) -> Vec<Change> {
    // Positions are dense (0, 1, 2, …): a slot per position
    let mut slots: Vec<Option<LoggedRef>> = Vec::with_capacity(messages.len());
    for event in events {
        let kind = event.kind.as_str();
        if !matches!(kind, "message" | "message.edited" | "message.retracted") {
            continue;
        }
        let Some(pos) = event.data.get("pos").and_then(Value::as_u64).map(|p| p as usize) else { continue };
        if slots.len() <= pos {
            slots.resize(pos + 1, None);
        }
        slots[pos] = match kind {
            "message.retracted" => None,
            _ => {
                let field = |key: &str| event.str(key).unwrap_or_default();
                Some((event.id.as_str(), field("key"), field("role"), field("text")))
            }
        };
    }
    compare(&slots, messages)
}

fn compare(logged: &[Option<LoggedRef>], messages: &[SentRef]) -> Vec<Change> {
    let mut changes = Vec::new();
    for (pos, &(role, text, id)) in messages.iter().enumerate() {
        match logged.get(pos).copied().flatten() {
            None => changes.push(Change::New(pos)),
            // Same message (same key, or the Engine's own answer coming back as text) with the
            // same text: nothing new
            Some((_, key, old_role, old_text))
                if old_text.trim() == text.trim()
                    && (old_role.eq_ignore_ascii_case(role) || key == message_key(&role.to_lowercase(), text, id)) => {}
            Some((event_id, ..)) => changes.push(Change::Edited(pos, event_id.to_string())),
        }
    }
    for (pos, slot) in logged.iter().enumerate().skip(messages.len()) {
        if let Some((event_id, ..)) = slot {
            changes.push(Change::Retracted(pos, event_id.to_string()));
        }
    }
    changes
}

/// The events for `changes`; `message(pos)` gives the client's message at a new or edited position
pub fn change_events(changes: Vec<Change>, message: impl Fn(usize) -> ClientMessage) -> Vec<NewEvent> {
    changes
        .into_iter()
        .map(|change| match change {
            Change::New(pos) => message_event("message", pos, &message(pos), json!({})),
            Change::Edited(pos, old) => message_event("message.edited", pos, &message(pos), json!({ "supersedes": old })),
            Change::Retracted(pos, old) => NewEvent::new("message.retracted", json!({ "pos": pos, "retracts": old })),
        })
        .collect()
}

/// The Engine's answer at `pos`
pub fn answer_event(text: &str, pos: usize) -> NewEvent {
    NewEvent::new(
        "message",
        json!({ "role": "assistant", "text": text, "pos": pos, "key": message_key("assistant", text, None), "source": "engine" }),
    )
}

/// A turn of the visible transcript whose answer has no card yet
#[derive(Debug, Clone, PartialEq)]
pub struct Unindexed {
    pub user_pos: u64,
    pub answer_pos: u64,
    pub user: String,
    pub answer: String,
}

/// Turns answered by the Engine that have neither a card (`card_enrichment`) nor a recorded
/// failure (`card_enrichment_failed`), oldest first. The log itself is the job list: after a
/// restart or a crash the missing cards are found again.
pub fn unindexed_turns(events: &[Event]) -> Vec<Unindexed> {
    let done: std::collections::HashSet<u64> = events
        .iter()
        .filter(|e| e.kind == "card_enrichment" || e.kind == "card_enrichment_failed")
        .filter_map(|e| e.data.get("answer_pos").and_then(Value::as_u64))
        .collect();
    let engine_answers: std::collections::HashSet<&str> =
        events.iter().filter(|e| e.kind == "message" && e.str("source") == Some("engine")).map(|e| e.id.as_str()).collect();
    let slots = transcript(events);
    slots
        .iter()
        .filter(|(pos, answer)| answer.role == "assistant" && engine_answers.contains(answer.event_id.as_str()) && !done.contains(pos))
        .filter(|(_, answer)| !answer.text.trim().is_empty())
        .filter_map(|(pos, answer)| {
            let user = slots.get(&pos.checked_sub(1)?)?;
            (user.role == "user").then(|| Unindexed { user_pos: pos - 1, answer_pos: *pos, user: user.text.clone(), answer: answer.text.clone() })
        })
        .collect()
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
    fn test_changes_from_events_match_diff() {
        let first = diff(&BTreeMap::new(), &[msg("user", "hi"), msg("assistant", "hello"), msg("user", "notes.txt padho")]);
        let events: Vec<Event> = first
            .iter()
            .enumerate()
            .map(|(i, e)| Event { v: 1, id: format!("ev{i}"), chat: "c".into(), seq: i as u64 + 1, ts: String::new(), kind: e.kind.clone(), data: e.data.clone() })
            .collect();
        // Role case does not count; an edit and a new message do
        let sent: Vec<SentRef> = vec![("User", "hi", None), ("assistant", "hello", None), ("user", "README padho", None), ("assistant", "ok", None)];
        assert_eq!(changes(&events, &sent), [Change::Edited(2, "ev2".into()), Change::New(3)]);
        assert_eq!(changes(&events, &sent[..1]), [Change::Retracted(1, "ev1".into()), Change::Retracted(2, "ev2".into())]);
    }

    #[test]
    fn test_the_engine_answer_coming_back_is_not_an_edit() {
        let mut events = diff(&BTreeMap::new(), &[msg("user", "hi")]);
        events.push(answer_event("Hello! ", 1));
        let log = logged(&events);
        assert!(diff(&log, &[msg("user", "hi"), msg("assistant", "Hello!")]).is_empty());
    }

    #[test]
    fn test_unindexed_turns_come_back_until_carded() {
        let mut events = diff(&BTreeMap::new(), &[msg("user", "hi")]);
        events.push(answer_event("Hello!", 1));
        events.extend(diff(&logged(&events), &[msg("user", "hi"), msg("assistant", "Hello!"), msg("user", "notes padho")]));
        events.push(answer_event("Done.", 3));
        let as_events = |events: &[NewEvent]| -> Vec<Event> {
            events
                .iter()
                .enumerate()
                .map(|(i, e)| Event { v: 1, id: format!("ev{i}"), chat: "c".into(), seq: i as u64 + 1, ts: String::new(), kind: e.kind.clone(), data: e.data.clone() })
                .collect()
        };
        let pending = unindexed_turns(&as_events(&events));
        assert_eq!(pending.iter().map(|t| t.answer_pos).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(pending[1].user, "notes padho");
        // One carded, one failed: nothing left
        events.push(NewEvent::new("card_enrichment", serde_json::json!({ "answer_pos": 1, "title": "x" })));
        events.push(NewEvent::new("card_enrichment_failed", serde_json::json!({ "answer_pos": 3 })));
        assert!(unindexed_turns(&as_events(&events)).is_empty());
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
