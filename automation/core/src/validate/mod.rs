//! `flows.validate`: every check in order (docs/automation/validation.md). Errors stop a save;
//! warnings do not.

mod graph;
pub mod uses;

pub use graph::expression_paths;

use crate::format::{self, check};
use crate::host::Catalog;
use crate::permissions::Needed;
use crate::problem::{pointer, Problem};
use crate::schema;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub valid: bool,
    pub problems: Vec<Problem>,
    pub permissions_needed: Value,
    /// What `permissions` does not grant (`null` when the flow has no `permissions` yet, or all is granted)
    pub permissions_missing: Value,
    pub outside_content: bool,
    pub normalized: Value,
    pub hash: String,
    #[serde(skip)]
    pub needed: Needed,
}

/// The hash of a flow's model: `sha256:<hex>`
pub fn hash(flow: &Value) -> String {
    let digest = Sha256::digest(flow.to_string().as_bytes());
    format!("sha256:{}", digest.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// `expected_name`: the file name without its suffix, when the flow comes from a file
pub fn validate(flow: &Value, expected_name: Option<&str>, catalog: &dyn Catalog) -> Report {
    let mut problems = Vec::new();
    check::check_flow(flow, &mut problems);
    if let (Some(expected), Some(name)) = (expected_name, flow.get("name").and_then(Value::as_str)) {
        if expected != name {
            problems.push(Problem::error("name_mismatch", "/name", format!("the file is named {expected} but `name` is {name}")).fix(Value::from(expected)));
        }
    }
    graph::check(flow, &mut problems);
    check_inputs(flow, &mut problems);
    check_secrets(flow, &mut problems);
    let needed = uses::check(flow, catalog, &mut problems);
    let granted = flow.get("permissions");
    let missing = granted.and_then(|p| needed.missing(p));
    if let Some(missing) = &missing {
        problems.push(Problem::warning("permission_needed", "/permissions", "the flow uses more than its permissions list; saving asks the owner to confirm it").fix(missing.to_value()));
    }
    let valid = problems.iter().all(|p| !p.is_error());
    Report {
        valid,
        problems,
        permissions_needed: needed.to_value(),
        permissions_missing: missing.map(|m| m.to_value()).unwrap_or(Value::Null),
        outside_content: needed.outside_content,
        normalized: normalize(flow),
        hash: hash(flow),
        needed,
    }
}

fn check_inputs(flow: &Value, out: &mut Vec<Problem>) {
    for (name, input) in flow.get("inputs").and_then(Value::as_object).into_iter().flatten() {
        if let Err(message) = schema::check_schema(input) {
            out.push(Problem::error("bad_inputs_schema", pointer("/inputs", name), message));
        }
    }
}

/// `env:NAME` must name a declared secret, and stands alone in a field that takes a secret
fn check_secrets(flow: &Value, out: &mut Vec<Problem>) {
    let declared: Vec<&str> = flow.get("secrets").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let mut check_ref = |value: &Value, at: String| {
        if let Some(name) = value.as_str().and_then(|t| t.strip_prefix("env:")) {
            if !declared.contains(&name) {
                out.push(Problem::error("unknown_secret", at, format!("`{name}` is not in the flow's `secrets`")).fix(json!({ "secrets": [name] })));
            }
        }
    };
    for (i, trigger) in flow.get("on").and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(secret) = trigger.get("webhook").and_then(|w| w.get("secret")) {
            check_ref(secret, pointer(&pointer(&pointer("/on", i), "webhook"), "secret"));
        }
    }
    let mut in_settings = Vec::new();
    let mut in_text = Vec::new();
    format::walk_steps(flow, &mut |step| {
        if let Some(settings) = step.map.get("settings").and_then(Value::as_object) {
            for (key, value) in settings {
                in_settings.push((value.clone(), pointer(&pointer(&step.pointer, "settings"), key)));
            }
        }
        if let Some(with) = step.map.get("with") {
            find_secret_text(with, &pointer(&step.pointer, "with"), &mut in_text);
        }
    });
    for (value, at) in in_settings {
        check_ref(&value, at);
    }
    for at in in_text {
        out.push(Problem::error("secret_in_text", at, "a secret is written into a step's input; secrets go only in fields that take them (a tool's settings, a webhook's secret)"));
    }
}

fn find_secret_text(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::String(text) if text.starts_with("env:") && text[4..].chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && text.len() > 4 => out.push(at.to_string()),
        Value::Array(items) => items.iter().enumerate().for_each(|(i, v)| find_secret_text(v, &pointer(at, i), out)),
        Value::Object(map) => map.iter().for_each(|(k, v)| find_secret_text(v, &pointer(at, k), out)),
        _ => {}
    }
}

/// The flow with the defaults filled in, so applications can show what will actually run
fn normalize(flow: &Value) -> Value {
    let Some(map) = flow.as_object() else { return flow.clone() };
    let mut out = map.clone();
    out.entry("on").or_insert_with(|| json!([]));
    out.entry("inputs").or_insert_with(|| json!({}));
    out.entry("secrets").or_insert_with(|| json!([]));
    out.entry("concurrency").or_insert_with(|| json!("queue"));
    out.entry("on_error").or_insert_with(|| json!([]));
    let limits = out.entry("limits").or_insert_with(|| json!({}));
    if let Some(limits) = limits.as_object_mut() {
        for (field, default) in crate::limits::defaults() {
            limits.entry(field.to_string()).or_insert(default);
        }
    }
    Value::Object(out.into_iter().collect::<Map<_, _>>())
}

#[cfg(test)]
mod tests;
