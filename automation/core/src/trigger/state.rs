//! What triggers keep between events, per flow, next to its other state
//! (`<flows>/state/<scope>/<flow>/`): `triggers.json` holds each trigger's last planned time,
//! counters, last error and poll answers; `dedupe.json` the `dedupe` values seen.

use super::{time_text, Owner};
use crate::run::state::{file, read_map, write_map, WRITING};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};

/// What a trigger counts (`flows.get` shows them)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Count {
    /// A run was created
    Runs,
    /// `filter` was false
    Filtered,
    /// A `dedupe` repeat
    Duplicate,
    /// Dropped by `concurrency: skip` or a full queue
    Skipped,
    /// A webhook without the right secret, or over its limits
    Refused,
    /// A poll that failed, a filter that could not be worked out, a run that could not be created
    Errors,
}

impl Count {
    pub fn as_str(self) -> &'static str {
        match self {
            Count::Runs => "runs",
            Count::Filtered => "filtered",
            Count::Duplicate => "duplicate",
            Count::Skipped => "skipped",
            Count::Refused => "refused",
            Count::Errors => "errors",
        }
    }
}

/// Every trigger's kept state of a flow, by trigger id
pub fn read(owner: &Owner) -> Map<String, Value> {
    read_map(&file(owner.paths, owner.scope, owner.name(), "triggers.json"))
}

pub fn of(owner: &Owner, id: &str) -> Value {
    read(owner).remove(id).unwrap_or_else(|| json!({}))
}

/// Changes one trigger's state; triggers of the same flow write one at a time
pub fn update(owner: &Owner, id: &str, change: impl FnOnce(&mut Map<String, Value>)) {
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(owner.paths, owner.scope, owner.name(), "triggers.json");
    let mut all = read_map(&path);
    let entry = all.entry(id.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    if let Some(map) = entry.as_object_mut() {
        change(map);
    }
    write_map(&path, &all);
}

pub fn count(owner: &Owner, id: &str, what: Count) {
    update(owner, id, |state| {
        let counts = state.entry("counts").or_insert_with(|| json!({}));
        let n = counts.get(what.as_str()).and_then(Value::as_u64).unwrap_or(0);
        counts[what.as_str()] = json!(n + 1);
        if what == Count::Runs {
            state.insert("last_event_at".into(), json!(time_text(Utc::now())));
        }
    });
}

/// A trigger that could not do its work (a poll failed): shown by `flows.get` until it works again
pub fn failed(owner: &Owner, id: &str, message: &str) {
    update(owner, id, |state| {
        let counts = state.entry("counts").or_insert_with(|| json!({}));
        let n = counts.get("errors").and_then(Value::as_u64).unwrap_or(0);
        counts["errors"] = json!(n + 1);
        state.insert("last_error".into(), json!({ "message": message, "at": time_text(Utc::now()) }));
    });
}

pub fn worked(owner: &Owner, id: &str) {
    if of(owner, id).get("last_error").is_some_and(|e| !e.is_null()) {
        update(owner, id, |state| {
            state.remove("last_error");
        });
    }
}

/// The last planned time a time trigger started a run (or began counting from)
pub fn last_planned(owner: &Owner, id: &str) -> Option<DateTime<Utc>> {
    let state = of(owner, id);
    let text = state.get("last_planned")?.as_str()?;
    DateTime::parse_from_rfc3339(text).ok().map(|t| t.with_timezone(&Utc))
}

pub fn set_last_planned(owner: &Owner, id: &str, time: DateTime<Utc>) {
    update(owner, id, |state| {
        state.insert("last_planned".into(), json!(time_text(time)));
    });
}

/// True the first time `key` is seen for this trigger within `keep_ms`; it is then remembered
pub fn first_time(owner: &Owner, id: &str, key: &str, keep_ms: i64, now: DateTime<Utc>) -> bool {
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let path = file(owner.paths, owner.scope, owner.name(), "dedupe.json");
    let mut seen = read_map(&path);
    let now_ms = now.timestamp_millis();
    seen.retain(|_, until| until.as_i64().is_some_and(|until| until > now_ms));
    let name = format!("{id}\u{1f}{key}");
    if seen.contains_key(&name) {
        return false;
    }
    seen.insert(name, json!(now_ms + keep_ms));
    write_map(&path, &seen);
    true
}
