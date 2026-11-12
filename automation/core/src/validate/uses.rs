//! What each step uses: nodes that must exist, the permissions the flow needs, and the warnings
//! about how it uses them (docs/automation/validation.md#warnings).

use crate::format::{self, StepRef};
use crate::host::{Catalog, NodeInfo};
use crate::permissions::{self, Needed};
use crate::problem::{closest, pointer, Problem};
use crate::{refs, schema};
use serde_json::Value;

/// A `repeat` with more rounds than this is worth a second look
const LARGE_REPEAT: u64 = 1_000;

/// A tool name split into the parts `tools.call` takes
#[derive(Debug, Clone, PartialEq)]
pub struct ToolName {
    pub kind: String,
    pub name: String,
    pub function: Option<String>,
}

/// `mcp:<server>/<tool>`, `plugin:<name>[/<function>]`, `skill:<name>`
pub fn parse_tool(text: &str) -> Result<ToolName, String> {
    let bad = || format!("`{text}` is not a tool name: use mcp:<server>/<tool>, plugin:<name>/<function> or skill:<name>");
    let (kind, rest) = text.split_once(':').ok_or_else(bad)?;
    let (name, function) = match rest.split_once('/') {
        Some((name, function)) => (name, Some(function.to_string())),
        None => (rest, None),
    };
    let ok_name = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    match (kind, &function) {
        ("mcp", Some(f)) if ok_name && !f.is_empty() => {}
        ("plugin", _) if ok_name => {}
        ("skill", None) if ok_name => {}
        _ => return Err(bad()),
    }
    Ok(ToolName { kind: kind.to_string(), name: name.to_string(), function })
}

/// Whether a flow gets outside content from its triggers (a webhook body, a poll's answer)
pub fn outside_triggers(flow: &Value) -> bool {
    flow.get("on").and_then(Value::as_array).into_iter().flatten().any(|t| t.get("webhook").is_some() || t.get("poll").is_some())
}

/// The outside-content rule (docs/automation/permissions.md#outside-content-and-ai-steps): a model
/// step that receives outside content, can read private data and can send data out. `tools` are
/// what the step's model may call.
pub fn untrusted_with_tools(outside: bool, tools: &[Option<NodeInfo>]) -> bool {
    let reaches_out = |info: &Option<NodeInfo>| info.as_ref().is_some_and(|i| i.network || i.sends);
    let sends = tools.iter().any(reaches_out);
    let receives_outside = outside || tools.iter().any(|i| i.as_ref().is_some_and(|i| i.network));
    // A tool not known to reach the network works on this computer: it can read private data
    let private = tools.iter().any(|i| !i.as_ref().is_some_and(|i| i.network));
    sends && receives_outside && private
}

fn literal(value: Option<&Value>) -> Option<&str> {
    value.and_then(Value::as_str).filter(|s| !s.contains("${{"))
}

pub fn check(flow: &Value, catalog: &dyn Catalog, out: &mut Vec<Problem>) -> Needed {
    let mut needed = Needed::default();
    let triggers: Vec<&Value> = flow.get("on").and_then(Value::as_array).into_iter().flatten().collect();
    needed.outside_content = triggers.iter().any(|t| t.get("webhook").is_some() || t.get("poll").is_some());
    let from_webhook = triggers.iter().any(|t| t.get("webhook").is_some());
    for (i, trigger) in triggers.iter().enumerate() {
        let here = pointer("/on", i);
        if let Some(path) = literal(trigger.get("file").and_then(|f| f.get("path"))) {
            needed.folder(path);
        }
        if let Some(poll) = trigger.get("poll") {
            if let Some(tool) = literal(poll.get("tool")) {
                tool_node(tool, &pointer(&pointer(&here, "poll"), "tool"), catalog, &mut needed, out);
            }
            if let Some(method) = literal(poll.get("call")) {
                method_node(method, &pointer(&pointer(&here, "poll"), "call"), catalog, &mut needed, out);
            }
        }
    }
    let mut referenced: Vec<String> = Vec::new();
    refs::references_in(flow, &mut referenced);
    super::graph::expression_paths(flow, &mut referenced);
    let mut after_wait = false;
    format::walk_steps(flow, &mut |step| {
        if matches!(step.kind.key, "wait" | "approve") || step.map.contains_key("approve_before") {
            after_wait = true;
        }
        let info = step_node(step, catalog, &mut needed, out);
        lint(step, info.as_ref(), from_webhook || after_wait, &referenced, out);
    });
    needed
}

fn step_node(step: &StepRef, catalog: &dyn Catalog, needed: &mut Needed, out: &mut Vec<Problem>) -> Option<NodeInfo> {
    let key = |field: &str| pointer(&step.pointer, field);
    match step.kind.key {
        "tool" => literal(step.map.get("tool")).and_then(|t| tool_node(t, &key("tool"), catalog, needed, out)),
        "call" => {
            let method = literal(step.map.get("call"))?;
            let info = method_node(method, &key("call"), catalog, needed, out);
            if method.starts_with("fs.") {
                if let Some(path) = literal(step.map.get("with").and_then(|w| w.get("path"))) {
                    needed.folder(path);
                }
            }
            info
        }
        "ask" | "agent" => {
            let model = literal(step.map.get("model"));
            needed.model(model);
            if let Some(model) = model {
                if catalog.model_installed(model) == Some(false) {
                    out.push(Problem::warning("model_not_installed", key("model"), format!("`{model}` is not installed now; the step fails until it is")));
                }
            }
            let mut infos = Vec::new();
            for (i, tool) in step.map.get("tools").and_then(Value::as_array).into_iter().flatten().enumerate() {
                if let Some(tool) = literal(Some(tool)) {
                    infos.push(tool_node(tool, &pointer(&key("tools"), i), catalog, needed, out));
                }
            }
            let allowed = step.map.get("allow_untrusted") == Some(&Value::Bool(true));
            if !infos.is_empty() && !allowed && untrusted_with_tools(needed.outside_content, &infos) {
                out.push(Problem::warning("untrusted_with_tools", step.pointer.clone(), "this model step receives outside content, can read private data and can send data out; it waits for the owner's approval before it runs (allow_untrusted: true removes that)"));
            }
            None
        }
        "approve" => {
            if let Some(tool) = literal(step.map.get("notify")) {
                tool_node(tool, &key("notify"), catalog, needed, out);
            }
            None
        }
        _ => None,
    }
}

fn tool_node(name: &str, at: &str, catalog: &dyn Catalog, needed: &mut Needed, out: &mut Vec<Problem>) -> Option<NodeInfo> {
    if let Err(message) = parse_tool(name) {
        out.push(Problem::error("invalid_value", at.to_string(), message));
        return None;
    }
    needed.tool(name);
    let found = catalog.tool(name);
    if found.is_none() && !catalog.names().is_empty() {
        out.push(unknown_node(name, at, catalog));
    }
    found
}

fn method_node(name: &str, at: &str, catalog: &dyn Catalog, needed: &mut Needed, out: &mut Vec<Problem>) -> Option<NodeInfo> {
    if permissions::not_callable(name) {
        out.push(Problem::error("not_callable", at.to_string(), format!("`{name}` cannot be called from a flow: a flow does not change its own rules, mint tokens or run programs")));
        return None;
    }
    needed.method(name);
    if permissions::ROUTES.contains(&name) {
        needed.scope("write");
        return None;
    }
    let found = catalog.method(name);
    match &found {
        Some(info) if info.scope.as_deref() == Some("admin") => {
            out.push(Problem::error("not_callable", at.to_string(), format!("`{name}` needs the admin scope, which is never granted to a flow")));
        }
        Some(info) => needed.scope(info.scope.as_deref().unwrap_or("read")),
        None if !catalog.names().is_empty() => out.push(unknown_node(name, at, catalog)),
        None => {}
    }
    found
}

fn unknown_node(name: &str, at: &str, catalog: &dyn Catalog) -> Problem {
    let names = catalog.names();
    let mut problem = Problem::error("unknown_node", at.to_string(), format!("`{name}` is not installed here or is not a method"));
    if let Some(close) = closest(name, names.iter().map(String::as_str)) {
        problem.message.push_str(&format!("; did you mean `{close}`?"));
        problem = problem.fix(Value::from(close));
    }
    problem
}

fn lint(step: &StepRef, info: Option<&NodeInfo>, needs_idempotency: bool, referenced: &[String], out: &mut Vec<Problem>) {
    let has = |field: &str| step.map.contains_key(field);
    if let Some(info) = info {
        if info.network && !has("retry") {
            out.push(Problem::warning("no_retry_network", step.pointer.clone(), "this step reaches the network; add retry so a passing failure does not fail the run").fix(serde_json::json!({ "retry": { "max": 3 } })));
        }
        if info.sends && needs_idempotency && !has("idempotency_key") {
            out.push(Problem::warning("no_idempotency", step.pointer.clone(), "this step sends something; with a webhook or a wait before it, add idempotency_key so it is never sent twice"));
        }
        if let (Some(schema_value), Some(with)) = (&info.input_schema, step.map.get("with")) {
            input_against_schema(with, schema_value, &pointer(&step.pointer, "with"), out);
            if let Some(properties) = schema_value.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    let is_code = property.get("x-cluaiz-code") == Some(&Value::Bool(true));
                    if is_code && with.get(name).and_then(Value::as_str).is_some_and(|t| t.contains("${{")) {
                        out.push(Problem::warning("reference_in_code", pointer(&pointer(&step.pointer, "with"), name), "a reference inside code or a command; pass the value as its own parameter"));
                    }
                }
            }
        }
    }
    if step.kind.key == "repeat" && step.map.get("max").and_then(Value::as_u64).is_some_and(|m| m > LARGE_REPEAT) {
        out.push(Problem::warning("large_repeat", pointer(&step.pointer, "max"), format!("more than {LARGE_REPEAT} rounds")));
    }
    let side_effect_free = step.kind.key == "ask" || info.is_some_and(|i| i.read_only);
    if step.explicit_id && !step.last && side_effect_free && !referenced.iter().any(|r| r == &format!("steps.{}", step.id) || r.starts_with(&format!("steps.{}.", step.id)) || r.starts_with(&format!("steps.{}[", step.id))) {
        out.push(Problem::warning("unused_output", step.pointer.clone(), format!("nothing reads steps.{} and the step changes nothing", step.id)));
    }
}

/// Literal input against the node's schema: missing required fields, unknown fields, wrong types.
/// Values with references are checked when the step runs.
fn input_against_schema(with: &Value, node_schema: &Value, at: &str, out: &mut Vec<Problem>) {
    let mut literal_only = with.clone();
    strip_references(&mut literal_only);
    for error in schema::validate(&literal_only, node_schema) {
        // A field whose value was a reference was removed above; its absence is not a problem
        if error.message == "is required" && with.pointer(&error.path).is_some() {
            continue;
        }
        out.push(Problem::error("invalid_input", format!("{at}{}", error.path), error.message));
    }
}

fn strip_references(value: &mut Value) {
    if let Value::Object(map) = value {
        map.retain(|_, v| !(matches!(v, Value::String(t) if t.contains("${{")) || v.is_object() && v.get("expr").is_some()));
        map.values_mut().for_each(strip_references);
    } else if let Value::Array(items) = value {
        items.iter_mut().for_each(strip_references);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names() {
        assert_eq!(parse_tool("mcp:github/create_issue").unwrap(), ToolName { kind: "mcp".into(), name: "github".into(), function: Some("create_issue".into()) });
        assert_eq!(parse_tool("plugin:pdf").unwrap().function, None);
        assert!(parse_tool("skill:code-review").is_ok());
        for bad in ["github", "mcp:github", "skill:a/b", "web:x", "mcp:Git Hub/x"] {
            assert!(parse_tool(bad).is_err(), "{bad}");
        }
    }
}
