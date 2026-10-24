//! Events of a chat's log. Every event is one JSON line: `v`, `id`, `chat`, `seq`, `ts`, `kind`
//! and the kind's own fields.
//!
//! Kinds: `message` (user, assistant or system), `message.edited`, `message.retracted`,
//! `tool_call`, `tool_result`, `approval`, `context_edit` (load, unload, compaction).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const EVENT_VERSION: u32 = 1;

/// A logged event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    pub id: String,
    pub chat: String,
    /// Position in the chat's log, from 1
    pub seq: u64,
    /// RFC 3339, UTC
    pub ts: String,
    pub kind: String,
    /// The kind's own fields (role, text, msg_id, tool, args, result, …)
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

/// An event to append; the store gives it its id, seq and time
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub kind: String,
    pub data: Map<String, Value>,
    /// When it happened (RFC 3339), for events imported from elsewhere; else the time of the append
    pub ts: Option<String>,
}

impl NewEvent {
    /// `kind` with the fields of `data` (a JSON object; anything else gives no fields)
    pub fn new(kind: &str, data: Value) -> Self {
        let data = match data {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        Self { kind: kind.to_string(), data, ts: None }
    }
}

impl Event {
    pub fn str(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(Value::as_str)
    }
}
