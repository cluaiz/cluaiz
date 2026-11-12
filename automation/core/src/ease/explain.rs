//! `flows.explain` in its plain form: a flow in words, made from the flow itself, with no model,
//! the same every time (docs/automation/methods/flows.explain.md).

use crate::expr::{op_def, Arity};
use crate::format::{walk_steps, StepRef};
use crate::host::Catalog;
use crate::trigger::{specs, Spec};
use crate::validate::Report;
use serde_json::{json, Map, Value};

/// Quoted texts (instructions, questions) are shortened to this many characters
const SHORT: usize = 80;
/// The summary names at most this many top-level steps
const SUMMARY_STEPS: usize = 6;

fn short(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= SHORT {
        return text;
    }
    let kept: String = text.chars().take(SHORT - 1).collect();
    format!("{}…", kept.trim_end())
}

fn quoted(text: &str) -> String {
    format!("\u{201c}{}\u{201d}", short(text))
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn names(list: &[String]) -> String {
    match list {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A value inside a condition, in words
fn value_words(value: &Value) -> String {
    match value {
        Value::Object(map) if map.len() == 1 && map.keys().next().is_some_and(|k| op_def(k).is_some()) => condition(value),
        Value::String(text) if text.contains("${{") => text.clone(),
        Value::String(text) => format!("\"{}\"", short(text)),
        Value::Array(items) => format!("[{}]", items.iter().map(value_words).collect::<Vec<_>>().join(", ")),
        other => short(&other.to_string()),
    }
}

/// A decision (`when`, `if`, `until`, `filter`) in words: `steps.order.output.total > 5000`
pub fn condition(value: &Value) -> String {
    let Some((op, args)) = value.as_object().filter(|m| m.len() == 1).and_then(|m| m.iter().next()) else {
        return match value {
            Value::Bool(true) => "always".into(),
            Value::Bool(false) => "never".into(),
            Value::String(text) => text.clone(),
            other => short(&other.to_string()),
        };
    };
    let Some(def) = op_def(op) else { return short(&value.to_string()) };
    let list: Vec<&Value> = match args {
        Value::Array(items) if def.arity != Arity::One => items.iter().collect(),
        other => vec![other],
    };
    let at = |i: usize| list.get(i).map(|a| value_words(a)).unwrap_or_default();
    let joined = |word: &str| list.iter().map(|a| inner(a)).collect::<Vec<_>>().join(word);
    match op.as_str() {
        "var" => list.first().and_then(|v| v.as_str()).unwrap_or_default().to_string(),
        "eq" => format!("{} is {}", at(0), at(1)),
        "ne" => format!("{} is not {}", at(0), at(1)),
        "gt" => format!("{} > {}", at(0), at(1)),
        "gte" => format!("{} ≥ {}", at(0), at(1)),
        "lt" => format!("{} < {}", at(0), at(1)),
        "lte" => format!("{} ≤ {}", at(0), at(1)),
        "all" if list.is_empty() => "always".into(),
        "any" if list.is_empty() => "never".into(),
        "all" => joined(" and "),
        "any" => joined(" or "),
        "not" => format!("not ({})", at(0)),
        "in" => format!("{} is one of {}", at(0), at(1)),
        _ => format!("{op}({})", list.iter().map(|a| value_words(a)).collect::<Vec<_>>().join(", ")),
    }
}

/// A condition inside `and` / `or`: brackets around the other kind
fn inner(value: &Value) -> String {
    let text = condition(value);
    match value.as_object().and_then(|m| m.keys().next()).map(String::as_str) {
        Some("all") | Some("any") => format!("({text})"),
        _ => text,
    }
}

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAY_NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

fn day(text: &str) -> Option<&'static str> {
    if let Ok(n) = text.parse::<usize>() {
        return DAYS.get(n % 7).copied();
    }
    DAY_NAMES.iter().position(|d| d.eq_ignore_ascii_case(text)).map(|i| DAYS[i])
}

/// `1-5` → Monday to Friday; `1,3` → Monday and Wednesday
fn days(field: &str) -> Option<String> {
    let mut parts = Vec::new();
    for item in field.split(',') {
        match item.split_once('-') {
            Some((a, b)) => parts.push(format!("{} to {}", day(a)?, day(b)?)),
            None => parts.push(day(item)?.to_string()),
        }
    }
    Some(names(&parts))
}

/// A cron line in words: `0 18 * * 5` → every Friday at 18:00. Lines it cannot say simply are
/// given as they are.
pub fn cron_words(line: &str) -> String {
    match line.trim() {
        "@hourly" => return "every hour".into(),
        "@daily" | "@midnight" => return "every day at 00:00".into(),
        "@weekly" => return "every Sunday at 00:00".into(),
        "@monthly" => return "on day 1 of every month at 00:00".into(),
        "@yearly" | "@annually" => return "every 1 January at 00:00".into(),
        _ => {}
    }
    let raw = format!("on the schedule `{}`", line.trim());
    let parts: Vec<&str> = line.split_whitespace().collect();
    let [minute, hour, dom, month, dow] = parts[..] else { return raw };
    let num = |s: &str| s.parse::<u32>().ok();
    let step = |s: &str| s.strip_prefix("*/").and_then(|n| n.parse::<u32>().ok());
    let (at_time, time) = match (num(minute), num(hour)) {
        (Some(m), Some(h)) => (true, format!("at {h:02}:{m:02}")),
        (Some(m), None) if hour == "*" => (false, format!("every hour at :{m:02}")),
        (Some(m), None) if step(hour).is_some() => (false, format!("every {} hours at :{m:02}", step(hour).unwrap_or(1))),
        (None, None) if minute == "*" && hour == "*" => (false, "every minute".into()),
        (None, None) if step(minute).is_some() && hour == "*" => (false, format!("every {} minutes", step(minute).unwrap_or(1))),
        _ => return raw,
    };
    let when = match (dom, month, dow) {
        ("*", "*", "*") => None,
        ("*", "*", dow) => match days(dow) {
            Some(days) => Some(format!("every {days}")),
            None => return raw,
        },
        (dom, "*", "*") if dom.split(',').all(|d| num(d).is_some()) => Some(format!("on day {} of every month", dom.replace(',', ", "))),
        _ => return raw,
    };
    match (when, at_time) {
        (None, true) => format!("every day {time}"),
        (None, false) => time,
        (Some(when), true) => format!("{when} {time}"),
        (Some(when), false) => format!("{time}, {when}"),
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn trigger_words(spec: &Spec, flow: &Value) -> String {
    let name = flow.get("name").and_then(Value::as_str).unwrap_or("this-flow");
    let body = spec.body();
    let mut out = match spec.kind {
        "manual" => "by hand".to_string(),
        "schedule" => {
            let zone = spec.beside("timezone").or_else(|| flow.get("timezone")).and_then(Value::as_str);
            let words = cron_words(body.as_str().unwrap_or_default());
            match zone {
                Some(zone) => format!("{words} ({zone})"),
                None => words,
            }
        }
        "every" => match spec.beside("starting").and_then(Value::as_str) {
            Some(start) => format!("every {}, starting {start}", text(Some(body))),
            None => format!("every {}", text(Some(body))),
        },
        "webhook" => format!("when a request with the flow's secret comes to /v1/hooks/{name}"),
        "poll" => {
            let what = spec.field("tool").or_else(|| spec.field("call")).and_then(Value::as_str).unwrap_or("a tool");
            let new = match spec.field("new_when") {
                None => "its answer changes".to_string(),
                Some(Value::String(s)) if s == "changed" => "its answer changes".to_string(),
                Some(Value::String(s)) if s == "new_items" => "its answer has new items".to_string(),
                Some(other) => format!("{} holds", condition(other)),
            };
            format!("every {}, {what} is checked; a run starts when {new}", text(spec.field("every")))
        }
        "file" => {
            let events: Vec<String> = spec.field("events").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_else(|| vec!["created".into()]);
            let glob = spec.field("glob").and_then(Value::as_str).filter(|g| *g != "*").map(|g| format!(" matching {g}")).unwrap_or_default();
            let deep = if spec.field("recursive") == Some(&Value::Bool(true)) { " or a folder inside it" } else { "" };
            format!("when a file{glob} is {} in {}{deep}", events.join(" or "), text(spec.field("path")))
        }
        "engine" => format!("when the Engine event {} happens", text(Some(body))),
        "event" => format!("when another flow sends the event {}", text(Some(body))),
        "tool" => format!("when an MCP client calls it as the tool flow.{name}"),
        "startup" => "when Automation starts".to_string(),
        other => other.to_string(),
    };
    if let Some(filter) = spec.beside("filter") {
        out.push_str(&format!(", if {}", condition(filter)));
    }
    if spec.beside("enabled") == Some(&Value::Bool(false)) {
        out.push_str(" (turned off)");
    }
    out
}

/// The names of `with`'s fields, when it is an object
fn with_fields(map: &Map<String, Value>) -> String {
    match map.get("with") {
        Some(Value::Object(with)) if !with.is_empty() => format!(" with {}", names(&with.keys().cloned().collect::<Vec<_>>())),
        _ => String::new(),
    }
}

fn node_words(name: &str, info: Option<String>) -> String {
    match info {
        Some(description) if !description.trim().is_empty() => format!("{name} ({})", short(description.trim().trim_end_matches('.'))),
        _ => name.to_string(),
    }
}

fn step_words(step: &StepRef, catalog: &dyn Catalog) -> String {
    let map = step.map;
    let value = map.get(step.kind.key).unwrap_or(&Value::Null);
    let field = |name: &str| map.get(name);
    let mut out = match step.kind.key {
        "tool" => {
            let name = text(Some(value));
            format!("Run {}{}", node_words(&name, catalog.tool(&name).and_then(|i| i.description)), with_fields(map))
        }
        "call" => {
            let name = text(Some(value));
            format!("Call {}{}", node_words(&name, catalog.method(&name).and_then(|i| i.description)), with_fields(map))
        }
        "ask" => {
            let mut out = format!("A model answers {}", quoted(&text(Some(value))));
            if field("output_schema").is_some() {
                out.push_str(" as JSON");
            }
            if let Some(model) = field("model").and_then(Value::as_str) {
                out.push_str(&format!(" (model {model})"));
            }
            out
        }
        "agent" => {
            let tools: Vec<String> = field("tools").and_then(Value::as_array).map(|l| l.iter().map(|t| text(Some(t))).collect()).unwrap_or_default();
            let turns = field("max_turns").and_then(Value::as_u64).unwrap_or(10);
            format!("A model works with {} for up to {turns} rounds: {}", names(&tools), quoted(&text(Some(value))))
        }
        "code" => format!("Run the WebAssembly module {} (function {})", text(Some(value)), field("fn").and_then(Value::as_str).unwrap_or("main")),
        "if" => format!("If {}", condition(value)),
        "switch" => format!("Choose the first of {} cases that holds", value.as_array().map_or(0, Vec::len)),
        "each" => {
            let mut out = format!("For each item of {} (as {})", text(Some(value)), field("as").and_then(Value::as_str).unwrap_or("item"));
            if let Some(n) = field("parallel").and_then(Value::as_u64).filter(|n| *n > 1) {
                out.push_str(&format!(", {n} at a time"));
            }
            out
        }
        "parallel" => {
            let branches: Vec<String> = value.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
            let mut out = format!("At the same time: {}", names(&branches));
            if field("wait").and_then(Value::as_str) == Some("any") {
                out.push_str(" (the first to finish wins)");
            }
            out
        }
        "repeat" => format!("Repeat until {}, at most {} rounds", field("until").map(condition).unwrap_or_default(), text(field("max"))),
        "wait" => match value {
            Value::Object(wait) if wait.contains_key("for") => format!("Wait {}", text(wait.get("for"))),
            Value::Object(wait) if wait.contains_key("until") => format!("Wait until {}", text(wait.get("until"))),
            Value::Object(wait) if wait.contains_key("event") => format!("Wait for the event {}", text(wait.get("event"))),
            Value::Object(wait) if wait.contains_key("webhook") => "Wait for a request to the run's own address".into(),
            other => format!("Wait {}", text(Some(other))),
        },
        "approve" => format!("Ask a person {}", quoted(&text(Some(value)))),
        "set" => format!("Keep {}", names(&value.as_object().map(|m| m.keys().cloned().collect::<Vec<_>>()).unwrap_or_default())),
        "run" => {
            let mut out = format!("Run the flow {}", text(Some(value)));
            if field("wait") == Some(&Value::Bool(false)) {
                out.push_str(" without waiting for it");
            }
            out
        }
        "emit" => format!("Send the event {}", text(Some(value))),
        "return" => match value.as_object() {
            Some(outputs) if !outputs.is_empty() => format!("End the run with {}", names(&outputs.keys().cloned().collect::<Vec<_>>())),
            _ => "End the run".into(),
        },
        "fail" => format!("Stop the run with the error {}", quoted(&text(Some(value)))),
        other => other.to_string(),
    };
    if let Some(question) = field("approve_before").and_then(Value::as_str) {
        out.push_str(&format!(", after a person says yes to {}", quoted(question)));
    }
    if let Some(max) = field("retry").and_then(|r| r.get("max")).and_then(Value::as_u64) {
        out.push_str(&format!(", tried again up to {max} times"));
    }
    if field("on_error").and_then(Value::as_str) == Some("continue") {
        out.push_str(", going on if it fails");
    }
    match field("when") {
        Some(when) => format!("Only when {}: {}", condition(when), lower_first(&out)),
        None => out,
    }
}

fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        // "A model…" stays as it is; a verb is lowered
        Some(first) if !text.starts_with("A ") => first.to_lowercase().chain(chars).collect(),
        _ => text.to_string(),
    }
}

/// The list a step is in, from its JSON Pointer: `steps`, `then`, `else`, `do`, a branch name,
/// `on_error`
fn list_of(pointer: &str) -> String {
    let parts: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
    match parts.len() {
        0 | 1 => String::new(),
        n => parts[n - 2].to_string(),
    }
}

/// What a flow reads, writes, sends, and which models it uses, in words
fn touches(flow: &Value, report: &Report, catalog: &dyn Catalog) -> Vec<String> {
    let needed = &report.needed;
    let mut out = Vec::new();
    let reads: Vec<String> = needed.methods.iter().filter(|m| catalog.method(m).is_some_and(|i| i.read_only)).cloned().collect();
    let changes: Vec<String> = needed.methods.iter().filter(|m| !catalog.method(m).is_some_and(|i| i.read_only)).cloned().collect();
    if !needed.tools.is_empty() {
        out.push(format!("Runs tools: {}", names(&needed.tools)));
    }
    if !reads.is_empty() {
        out.push(format!("Reads with: {}", names(&reads)));
    }
    if !changes.is_empty() {
        out.push(format!("Changes things with: {}", names(&changes)));
    }
    let sends: Vec<String> = needed.tools.iter().filter(|t| catalog.tool(t).is_some_and(|i| i.sends || i.network)).cloned().collect();
    if !sends.is_empty() {
        out.push(format!("Sends data out with: {}", names(&sends)));
    }
    if !needed.folders.is_empty() {
        out.push(format!("Folders: {}", names(&needed.folders)));
    }
    let network: Vec<String> = flow.get("permissions").and_then(|p| p.get("network")).and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default();
    if !network.is_empty() {
        out.push(format!("Network for code steps: {}", names(&network)));
    }
    if !needed.models.is_empty() {
        let models: Vec<String> = needed.models.iter().map(|m| if m == "*" { "the default chat model".to_string() } else { m.clone() }).collect();
        out.push(format!("Models: {}", names(&models)));
    }
    if needed.outside_content {
        out.push("Reads content from outside (a webhook, a web page, mail): it is data, never instructions".into());
    }
    out
}

/// The plain explanation: `{ summary, triggers, steps: [{ id, text, depth, list }], touches, warnings }`
pub fn explain(flow: &Value, report: &Report, catalog: &dyn Catalog) -> Value {
    let mut triggers: Vec<String> = specs(flow).iter().map(|s| capital(&trigger_words(s, flow))).collect();
    if triggers.is_empty() {
        triggers.push("By hand".into());
    }
    let mut steps = Vec::new();
    let mut top = Vec::new();
    walk_steps(flow, &mut |step| {
        let text = step_words(step, catalog);
        let list = list_of(&step.pointer);
        if step.around.is_empty() && list == "steps" {
            top.push(lower_first(&text));
        }
        steps.push(json!({ "id": step.id, "text": text, "depth": step.around.len(), "list": list }));
    });
    let mut shown: Vec<String> = top.iter().take(SUMMARY_STEPS).cloned().collect();
    if top.len() > SUMMARY_STEPS {
        shown.push(format!("{} more steps", top.len() - SUMMARY_STEPS));
    }
    let start = triggers.iter().map(|t| lower_first(t)).collect::<Vec<_>>().join(" or ");
    let summary = capital(&format!("{start} → {}.", shown.join(" → ")));
    let warnings: Vec<String> = report.problems.iter().filter(|p| !p.is_error()).map(|p| if p.path.is_empty() { p.message.clone() } else { format!("{}: {}", p.path, p.message) }).collect();
    json!({ "summary": summary, "triggers": triggers, "steps": steps, "touches": touches(flow, report, catalog), "warnings": warnings })
}
