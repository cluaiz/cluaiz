//! Triggers: what starts a run (docs/automation/triggers.md). This module is the part without
//! clocks, folders or HTTP: which triggers a flow has, cron times and missed times, the checks
//! every event passes (`filter`, `dedupe`, `inputs`), poll answers, file names and webhook
//! signatures. Listening is the host's work.

pub mod cron;
pub mod files;
pub mod poll;
pub mod state;
pub mod timing;
pub mod webhook;

use crate::error::FlowError;
use crate::expr::values::Zone;
use crate::journal::Paths;
use crate::refs;
use crate::run::Context;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

/// Kinds that start runs in this release. `tool` is checked and saved, and listens once flows
/// are offered as tools.
pub const BUILT: &[&str] = &["manual", "schedule", "every", "webhook", "poll", "file", "engine", "event", "tool", "startup"];

/// `every` and `poll.every` are at least this long (*proposed*: 1 minute)
pub const MIN_INTERVAL_MS: i64 = 60_000;

/// How long a `dedupe` value is remembered when `dedupe_for` is absent (*proposed*: 24 hours)
pub const DEDUPE_FOR_MS: i64 = 24 * 3_600_000;

/// One item of `on`
#[derive(Debug, Clone)]
pub struct Spec {
    /// Given, or the kind and its place: `schedule_0`
    pub id: String,
    pub kind: &'static str,
    /// The whole item: the kind key and the fields every trigger may have
    pub value: Map<String, Value>,
}

impl Spec {
    /// The value under the kind key: a cron line, a duration, an event name or an object
    pub fn body(&self) -> &Value {
        self.value.get(self.kind).unwrap_or(&Value::Null)
    }

    /// A field of an object kind (`webhook.secret`, `poll.every`)
    pub fn field(&self, name: &str) -> Option<&Value> {
        self.body().get(name)
    }

    /// A field beside the kind key (`filter`, `timezone`, `catch_up`)
    pub fn beside(&self, name: &str) -> Option<&Value> {
        self.value.get(name)
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.field(name).and_then(Value::as_str)
    }

    pub fn enabled(&self) -> bool {
        self.value.get("enabled").and_then(Value::as_bool).unwrap_or(true)
    }

    pub fn built(&self) -> bool {
        BUILT.contains(&self.kind)
    }
}

/// The triggers of a flow, in the order of `on`
pub fn specs(flow: &Value) -> Vec<Spec> {
    let kinds: Vec<&'static str> = crate::format::tables::TRIGGERS.iter().map(|t| t.key).collect();
    flow.get("on")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, item)| {
            let value = item.as_object()?.clone();
            let kind = *kinds.iter().find(|k| value.contains_key(**k))?;
            let id = value.get("id").and_then(Value::as_str).map(String::from).unwrap_or_else(|| format!("{kind}_{i}"));
            Some(Spec { id, kind, value })
        })
        .collect()
}

/// What an event becomes after the checks every trigger has
#[derive(Debug, Clone, PartialEq)]
pub enum Admit {
    /// A run starts with this `trigger` and these inputs
    Run { trigger: Value, inputs: Value },
    /// `filter` was false
    Filtered,
    /// The `dedupe` value was seen within `dedupe_for`
    Duplicate,
}

/// The flow a trigger belongs to, for its kept state
pub struct Owner<'a> {
    pub paths: &'a Paths,
    pub scope: &'a str,
    pub flow: &'a Value,
}

impl Owner<'_> {
    pub fn name(&self) -> &str {
        self.flow.get("name").and_then(Value::as_str).unwrap_or_default()
    }
}

pub fn time_text(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// `filter`, then `dedupe`, then `inputs` (docs/automation/triggers.md#fields-every-trigger-may-have).
/// `data` is what the kind brought; `kind`, `id` and `at` are added to it.
pub fn admit(owner: &Owner, spec: &Spec, data: Value, now: DateTime<Utc>) -> Result<Admit, FlowError> {
    let mut trigger = match data {
        Value::Object(map) => map,
        Value::Null => Map::new(),
        other => Map::from_iter([("data".to_string(), other)]),
    };
    trigger.insert("kind".into(), json!(spec.kind));
    trigger.insert("id".into(), json!(spec.id));
    trigger.entry("at").or_insert_with(|| json!(time_text(now)));
    let trigger = Value::Object(trigger);
    let zone = Zone::parse(owner.flow.get("timezone").and_then(Value::as_str)).unwrap_or(Zone::Local);
    let mut ctx = Context::new(Value::Null, trigger.clone(), Value::Null, zone, Map::new(), owner.paths.flows.clone());
    if let Some(filter) = spec.beside("filter") {
        if !ctx.decide(filter)? {
            return Ok(Admit::Filtered);
        }
    }
    if let Some(dedupe) = spec.beside("dedupe") {
        let key = refs::to_text(&ctx.resolve(dedupe)?);
        let keep_ms = spec.beside("dedupe_for").and_then(Value::as_str).and_then(|d| crate::units::parse_duration(d).ok()).unwrap_or(DEDUPE_FOR_MS);
        if !state::first_time(owner, &spec.id, &key, keep_ms, now) {
            return Ok(Admit::Duplicate);
        }
    }
    let inputs = match spec.beside("inputs") {
        Some(inputs) => ctx.resolve(inputs)?,
        // A flow called as a tool: the caller's arguments are its inputs
        None if spec.kind == "tool" => trigger.get("arguments").cloned().unwrap_or(Value::Null),
        None => Value::Null,
    };
    Ok(Admit::Run { trigger, inputs })
}

#[cfg(test)]
mod tests;
