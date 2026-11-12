//! `poll` triggers: run a tool or method every N minutes; a run starts when its answer has
//! something new (docs/automation/triggers.md#poll).

use super::Spec;
use crate::error::{FlowError, Kind};
use crate::expr::values::{equal, Zone};
use crate::host::{RunInfo, StepHost};
use crate::journal::Paths;
use crate::refs;
use crate::run::kinds::{method_allowed, tool_allowed, tool_output, tools_call_params};
use crate::run::Context;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Item keys a poll remembers, the newest (*proposed*: 10,000)
pub const MAX_SEEN: usize = 10_000;

/// Runs the poll's tool or method once, as the flow (its permissions apply)
pub async fn poll_once(host: &dyn StepHost, run: &RunInfo, spec: &Spec) -> Result<Value, FlowError> {
    let with = spec.field("with").cloned().unwrap_or_else(|| json!({}));
    if let Some(name) = spec.text("tool") {
        let tool = tool_allowed(&run.permissions, name)?;
        let raw = host.call(run, "tools.call", tools_call_params(&tool, with)).await?;
        return tool_output(&tool.kind, raw);
    }
    let method = spec.text("call").ok_or_else(|| FlowError::new(Kind::InvalidInput, "a poll has `tool` or `call`"))?;
    method_allowed(&run.permissions, method)?;
    host.call(run, method, with).await
}

/// What makes an item the same item: the value at `key`, or the whole item
fn item_key(item: &Value, key: Option<&str>) -> String {
    let value = match key.map(refs::parse_path) {
        Some(Ok(segments)) => refs::lookup(item, &segments),
        _ => item.clone(),
    };
    match value {
        Value::String(text) => text,
        Value::Number(n) => n.to_string(),
        other => {
            let digest = Sha256::digest(serde_json::to_vec(&other).unwrap_or_default());
            digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
        }
    }
}

fn items_of(output: &Value, path: Option<&str>) -> Result<Vec<Value>, FlowError> {
    let list = match path {
        Some(path) => refs::lookup(output, &refs::parse_path(path).map_err(|m| FlowError::new(Kind::Expression, m))?),
        None => output.clone(),
    };
    match list {
        Value::Array(items) => Ok(items),
        _ => Err(FlowError::new(Kind::InvalidOutput, format!("the poll's answer has no list at `{}`", path.unwrap_or("its top")))),
    }
}

/// The answer compared with the one kept: the trigger data when there is something new, and what
/// to keep for next time (`{ previous, seen }`)
pub fn compare(spec: &Spec, output: &Value, kept: &Value, paths: &Paths, zone: Zone) -> Result<(Option<Value>, Value), FlowError> {
    let new_when = spec.field("new_when").cloned().unwrap_or_else(|| json!("changed"));
    let key = spec.text("key");
    let items_at = spec.text("items");
    let first = kept.get("previous").is_none();
    let previous = kept.get("previous").cloned().unwrap_or(Value::Null);
    let mut seen: Vec<String> = kept.get("seen").and_then(Value::as_array).map(|s| s.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    let mut new_items = Value::Null;
    let is_new = match new_when.as_str() {
        Some("changed") => !equal(output, &previous),
        Some("new_items") => {
            let mut known: HashSet<String> = seen.iter().cloned().collect();
            let fresh: Vec<Value> = items_of(output, items_at)?.into_iter().filter(|item| known.insert(item_key(item, key))).collect();
            seen.extend(fresh.iter().map(|item| item_key(item, key)));
            if seen.len() > MAX_SEEN {
                seen.drain(..seen.len() - MAX_SEEN);
            }
            let any = !fresh.is_empty();
            new_items = Value::Array(fresh);
            any
        }
        Some(other) => return Err(FlowError::new(Kind::InvalidInput, format!("`new_when: {other}`: use changed, new_items or an expression"))),
        None => {
            let trigger = json!({ "kind": "poll", "id": spec.id, "output": output, "previous": previous });
            let mut ctx = Context::new(Value::Null, trigger, Value::Null, zone, Map::new(), paths.flows.clone());
            ctx.decide(&new_when)?
        }
    };
    let keep = json!({ "previous": output, "seen": seen });
    let run_first = spec.text("first") == Some("run");
    let data = (is_new && (!first || run_first)).then(|| json!({ "output": output, "previous": previous, "new_items": new_items }));
    Ok((data, keep))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(poll: Value) -> Spec {
        super::super::specs(&json!({ "on": [{ "poll": poll }] })).remove(0)
    }

    fn paths() -> Paths {
        Paths { flows: std::env::temp_dir() }
    }

    #[test]
    fn new_items_by_key() {
        let s = spec(json!({ "tool": "mcp:mail/unread", "every": "5m", "new_when": "new_items", "items": "mails", "key": "id" }));
        let one = json!({ "mails": [{ "id": "a" }, { "id": "b" }] });
        let (data, kept) = compare(&s, &one, &json!({}), &paths(), Zone::Utc).unwrap();
        assert!(data.is_none(), "the first answer is only remembered");
        let two = json!({ "mails": [{ "id": "b" }, { "id": "c" }] });
        let (data, kept) = compare(&s, &two, &kept, &paths(), Zone::Utc).unwrap();
        assert_eq!(data.unwrap()["new_items"], json!([{ "id": "c" }]));
        let (data, _) = compare(&s, &two, &kept, &paths(), Zone::Utc).unwrap();
        assert!(data.is_none());
    }

    #[test]
    fn changed_first_run_and_expressions() {
        let s = spec(json!({ "call": "models.list", "every": "1m", "first": "run" }));
        let (data, kept) = compare(&s, &json!([1]), &json!({}), &paths(), Zone::Utc).unwrap();
        assert_eq!(data.unwrap()["previous"], Value::Null);
        assert!(compare(&s, &json!([1]), &kept, &paths(), Zone::Utc).unwrap().0.is_none());
        assert!(compare(&s, &json!([1, 2]), &kept, &paths(), Zone::Utc).unwrap().0.is_some());
        let rule = spec(json!({ "call": "x.y", "every": "1m", "new_when": { "gt": [{ "var": "trigger.output.n" }, { "var": "trigger.previous.n" }] } }));
        let kept = json!({ "previous": { "n": 5 } });
        assert!(compare(&rule, &json!({ "n": 6 }), &kept, &paths(), Zone::Utc).unwrap().0.is_some());
        assert!(compare(&rule, &json!({ "n": 4 }), &kept, &paths(), Zone::Utc).unwrap().0.is_none());
        let bad = spec(json!({ "call": "x.y", "every": "1m", "new_when": "new_items", "items": "list" }));
        assert!(compare(&bad, &json!({ "list": 3 }), &json!({}), &paths(), Zone::Utc).is_err());
    }
}
