//! The structure check: every field against the tables (unknown fields, missing fields, types,
//! allowed values, durations, step kinds, expression shapes). Meaning (ids, references, nodes,
//! permissions) is checked by `validate`.

use super::tables::{self, Field, Ty};
use super::{step_kind, KindError};
use crate::expr::{self, values::Zone};
use crate::problem::{closest, pointer, Problem};
use crate::{refs, units};
use serde_json::{Map, Value};

static NAME: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^[a-z0-9][a-z0-9_-]{0,63}$").unwrap_or_else(|_| unreachable!()));
static ID: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^[A-Za-z][A-Za-z0-9_]{0,63}$").unwrap_or_else(|_| unreachable!()));

pub fn valid_name(name: &str) -> bool {
    NAME.is_match(name)
}

pub fn valid_id(id: &str) -> bool {
    ID.is_match(id)
}

pub fn check_flow(flow: &Value, out: &mut Vec<Problem>) {
    let Some(map) = flow.as_object() else {
        out.push(Problem::error("wrong_type", "", "a flow is an object (a YAML mapping)"));
        return;
    };
    if let Some(version) = map.get("flow").and_then(Value::as_str) {
        if !tables::VERSIONS.contains(&version) {
            out.push(Problem::error("unknown_version", "/flow", format!("`{version}` is not a format this release reads; use {}", tables::VERSION)).fix(Value::from(tables::VERSION)));
        }
    }
    if let Some(name) = map.get("name").and_then(Value::as_str) {
        if !valid_name(name) {
            out.push(Problem::error("invalid_value", "/name", "a name is 1–64 characters: a-z, 0-9, - and _, starting with a letter or digit"));
        }
    }
    if let Some(zone) = map.get("timezone").and_then(Value::as_str) {
        if let Err(message) = Zone::parse(Some(zone)) {
            out.push(Problem::error("bad_timezone", "/timezone", message));
        }
    }
    if map.get("steps").and_then(Value::as_array).is_some_and(Vec::is_empty) {
        out.push(Problem::error("invalid_value", "/steps", "a flow needs at least one step"));
    }
    check_fields(map, tables::FLOW, &[], "", out);
}

fn check_fields(map: &Map<String, Value>, fields: &[Field], also: &[&Field], path: &str, out: &mut Vec<Problem>) {
    let known = || fields.iter().chain(also.iter().copied());
    for field in known() {
        match map.get(field.name) {
            None if field.required => out.push(Problem::error("missing_field", path.to_string(), format!("`{}` is required: {}", field.name, field.desc))),
            None => {}
            Some(value) => check_value(value, &field.ty, &pointer(path, field.name), out),
        }
    }
    for key in map.keys() {
        if known().all(|f| f.name != key) {
            let names: Vec<&str> = known().map(|f| f.name).collect();
            let mut problem = Problem::error("unknown_field", pointer(path, key), format!("`{key}` is not a field here"));
            if let Some(close) = closest(key, names.iter().copied()) {
                problem.message.push_str(&format!("; did you mean `{close}`?"));
                problem = problem.fix(Value::from(close));
            }
            out.push(problem);
        }
    }
}

fn wrong(path: &str, wanted: &str, value: &Value, out: &mut Vec<Problem>) {
    out.push(Problem::error("wrong_type", path.to_string(), format!("expected {wanted}, found {}", expr::values::describe(value))));
}

pub fn check_value(value: &Value, ty: &Ty, path: &str, out: &mut Vec<Problem>) {
    match ty {
        Ty::Text => match value {
            Value::String(text) => check_text(text, path, out),
            other => wrong(path, "text", other, out),
        },
        Ty::Bool if !value.is_boolean() => wrong(path, "true or false", value, out),
        Ty::Int if !(value.is_i64() || value.is_u64()) => wrong(path, "a whole number", value, out),
        Ty::Number if !value.is_number() => wrong(path, "a number", value, out),
        Ty::Duration => match value.as_str() {
            Some(text) if text.contains("${{") => check_text(text, path, out),
            Some(text) => {
                if let Err(message) = units::parse_duration(text).and_then(|ms| if ms < 0 { Err("a duration here cannot be negative".into()) } else { Ok(ms) }) {
                    out.push(Problem::error("bad_duration", path.to_string(), message));
                }
            }
            None => wrong(path, "a duration such as 30s or 10m", value, out),
        },
        Ty::Size => match value.as_str().map(units::parse_size) {
            Some(Ok(_)) => {}
            Some(Err(message)) => out.push(Problem::error("invalid_value", path.to_string(), message)),
            None => wrong(path, "a size such as 512KB or 4MB", value, out),
        },
        Ty::Time => match value.as_str() {
            Some(text) if text.contains("${{") || expr::values::parse_time(text).is_some() => check_text(text, path, out),
            Some(text) => out.push(Problem::error("bad_time", path.to_string(), format!("\"{text}\" is not an RFC 3339 time such as 2026-12-01T09:00:00+05:30"))),
            None => wrong(path, "a time", value, out),
        },
        Ty::Expr => check_expression(value, path, true, out),
        Ty::Any => check_any(value, path, out),
        Ty::Enum(allowed) => match value.as_str() {
            Some(text) if allowed.contains(&text) => {}
            _ => {
                let mut problem = Problem::error("invalid_value", path.to_string(), format!("expected one of {}", allowed.join(", ")));
                if let Some(close) = value.as_str().and_then(|t| closest(t, allowed.iter().copied())) {
                    problem = problem.fix(Value::from(close));
                }
                out.push(problem);
            }
        },
        Ty::List(item) => match value.as_array() {
            Some(items) => items.iter().enumerate().for_each(|(i, v)| check_value(v, item, &pointer(path, i), out)),
            None => wrong(path, "a list", value, out),
        },
        Ty::Object(fields) => match value.as_object() {
            Some(map) => check_fields(map, fields, &[], path, out),
            None => wrong(path, "an object", value, out),
        },
        Ty::Map(item) => match value.as_object() {
            Some(map) => map.iter().for_each(|(k, v)| check_value(v, item, &pointer(path, k), out)),
            None => wrong(path, "an object", value, out),
        },
        Ty::Steps => match value.as_array() {
            Some(steps) => steps.iter().enumerate().for_each(|(i, s)| check_step(s, &pointer(path, i), out)),
            None => wrong(path, "a list of steps", value, out),
        },
        Ty::Triggers => match value.as_array() {
            Some(triggers) => triggers.iter().enumerate().for_each(|(i, t)| check_trigger(t, &pointer(path, i), out)),
            None => wrong(path, "a list of triggers", value, out),
        },
        Ty::Schema if !value.is_object() => wrong(path, "a JSON Schema object", value, out),
        Ty::Open | Ty::Schema => {}
        Ty::OneOf(options) => {
            let fits = options.iter().any(|option| {
                let mut trial = Vec::new();
                check_value(value, option, path, &mut trial);
                trial.is_empty()
            });
            if !fits {
                // Report against the option of the same JSON type, so the message is specific
                let same = options.iter().find(|o| same_json_type(value, o)).unwrap_or(&options[0]);
                check_value(value, same, path, out);
            }
        }
        _ => {}
    }
}

fn same_json_type(value: &Value, ty: &Ty) -> bool {
    matches!(
        (value, ty),
        (Value::String(_), Ty::Text | Ty::Enum(_) | Ty::Duration | Ty::Size | Ty::Time)
            | (Value::Array(_), Ty::List(_) | Ty::Steps | Ty::Triggers)
            | (Value::Object(_), Ty::Object(_) | Ty::Map(_) | Ty::Schema | Ty::Open)
    )
}

/// References in text must be well formed, and secrets are never readable as values
fn check_text(text: &str, path: &str, out: &mut Vec<Problem>) {
    if !text.contains("${{") {
        return;
    }
    match refs::split_template(text) {
        Err(message) => out.push(Problem::error("invalid_value", path.to_string(), message)),
        Ok(pieces) => {
            for piece in pieces {
                if let refs::Piece::Ref(reference) = piece {
                    if let Err(message) = refs::parse_path(&reference) {
                        out.push(Problem::error("invalid_value", path.to_string(), message));
                    } else if reference == "env" || reference.starts_with("env.") || reference.starts_with("env[") {
                        out.push(Problem::error("secret_in_text", path.to_string(), "secrets cannot be read with ${{ }}; name them in `secrets` and write env:NAME in a field that takes a secret"));
                    }
                }
            }
        }
    }
}

/// Plain values: texts are checked for references, `{ expr: … }` for its expression
fn check_any(value: &Value, path: &str, out: &mut Vec<Problem>) {
    match value {
        Value::String(text) => check_text(text, path, out),
        Value::Array(items) => items.iter().enumerate().for_each(|(i, v)| check_any(v, &pointer(path, i), out)),
        Value::Object(map) if map.len() == 1 && map.contains_key("expr") => check_expression(&map["expr"], &pointer(path, "expr"), false, out),
        Value::Object(map) => map.iter().for_each(|(k, v)| check_any(v, &pointer(path, k), out)),
        _ => {}
    }
}

/// `decision`: the value decides (`when`, `if`, `filter`) and must be a boolean or an operator
pub fn check_expression(value: &Value, path: &str, decision: bool, out: &mut Vec<Problem>) {
    if decision {
        match value {
            Value::Bool(_) => return,
            Value::Object(map) if expr::operator_of(value).is_none() => {
                let key = map.keys().next().map(String::as_str).unwrap_or_default();
                let mut problem = Problem::error("unknown_operator", path.to_string(), format!("`{key}` is not an operator"));
                if let Some(close) = expr::close_operator(key) {
                    problem.message.push_str(&format!("; did you mean `{close}`?"));
                    problem = problem.fix(Value::from(close));
                }
                out.push(problem);
                return;
            }
            Value::Object(_) => {}
            other => return wrong(path, "true, false or an expression", other, out),
        }
    }
    expr::check_shape(value, path, &mut |at, code, message| out.push(Problem::error(code, at.to_string(), message)));
}

fn check_step(step: &Value, path: &str, out: &mut Vec<Problem>) {
    let Some(map) = step.as_object() else {
        return wrong(path, "a step (an object)", step, out);
    };
    let kind = match step_kind(map) {
        Ok(kind) => kind,
        Err(KindError::None) => {
            let kinds: Vec<&str> = tables::KINDS.iter().map(|k| k.key).collect();
            let mut problem = Problem::error("no_kind", path.to_string(), format!("a step needs one kind key: {}", kinds.join(", ")));
            if let Some(close) = map.keys().find_map(|k| closest(k, kinds.iter().copied())) {
                problem.message.push_str(&format!("; did you mean `{close}`?"));
                problem = problem.fix(Value::from(close));
            }
            out.push(problem);
            return;
        }
        Err(KindError::Many(keys)) => {
            out.push(Problem::error("many_kinds", path.to_string(), format!("a step has one kind; this one has {}", keys.join(" and "))));
            return;
        }
    };
    if let Some(id) = map.get("id").and_then(Value::as_str) {
        if !valid_id(id) {
            out.push(Problem::error("invalid_id", pointer(path, "id"), format!("`{id}`: an id is 1–64 letters, digits and _, starting with a letter")));
        }
    }
    if kind.key == "repeat" && !map.contains_key("max") {
        out.push(Problem::error("repeat_without_max", path.to_string(), "repeat needs `max`: there is no repeat without a limit"));
    }
    if kind.key == "switch" {
        check_switch(map, path, out);
    }
    let key_field = Field { name: kind.key, ty: kind.ty, required: true, desc: kind.desc };
    let mut also: Vec<&Field> = vec![&key_field];
    // A kind's own field with a common name (`with`, `timeout`) keeps the common check
    also.extend(kind.fields.iter().filter(|f| tables::COMMON.iter().all(|c| c.name != f.name) && !(kind.key == "repeat" && f.name == "max" && !map.contains_key("max"))));
    check_fields(map, tables::COMMON, &also, path, out);
}

fn check_switch(map: &Map<String, Value>, path: &str, out: &mut Vec<Problem>) {
    let short = map.contains_key("match");
    for (i, case) in map.get("switch").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let here = pointer(&pointer(path, "switch"), i);
        let (has_when, has_is) = (case.get("when").is_some(), case.get("is").is_some());
        if short && !has_is || !short && !has_when || has_when && has_is {
            out.push(Problem::error("invalid_value", here, if short { "with `match`, each case has `is` (and no `when`)" } else { "each case has `when` (or use `match` with `is`)" }));
        }
    }
}

fn check_trigger(trigger: &Value, path: &str, out: &mut Vec<Problem>) {
    let Some(map) = trigger.as_object() else {
        return wrong(path, "a trigger (an object)", trigger, out);
    };
    let present: Vec<&tables::TriggerDef> = tables::TRIGGERS.iter().filter(|t| map.contains_key(t.key)).collect();
    let def = match present.as_slice() {
        [one] => *one,
        [] => {
            let kinds: Vec<&str> = tables::TRIGGERS.iter().map(|t| t.key).collect();
            out.push(Problem::error("no_kind", path.to_string(), format!("a trigger needs one kind key: {}", kinds.join(", "))));
            return;
        }
        many => {
            out.push(Problem::error("many_kinds", path.to_string(), format!("a trigger has one kind; this one has {}", many.iter().map(|t| t.key).collect::<Vec<_>>().join(" and "))));
            return;
        }
    };
    if def.key == "webhook" && map.get("webhook").and_then(|w| w.get("secret")).is_none() {
        out.push(Problem::error("webhook_without_secret", pointer(path, "webhook"), "a webhook needs `secret: env:NAME`, so only callers with the secret start the flow"));
    }
    if def.key == "schedule" {
        if let Some(cron) = map.get("schedule").and_then(Value::as_str) {
            if let Err(message) = check_cron(cron) {
                out.push(Problem::error("bad_cron", pointer(path, "schedule"), message));
            }
        }
        if let Some(zone) = map.get("timezone").and_then(Value::as_str) {
            if let Err(message) = Zone::parse(Some(zone)) {
                out.push(Problem::error("bad_timezone", pointer(path, "timezone"), message));
            }
        }
    }
    let poll = map.get("poll").and_then(Value::as_object);
    let intervals = [(map.get("every"), pointer(path, "every")), (poll.and_then(|p| p.get("every")), pointer(&pointer(path, "poll"), "every"))];
    for (value, at) in intervals {
        if let Some(Ok(ms)) = value.and_then(Value::as_str).map(crate::units::parse_duration) {
            if ms < crate::trigger::MIN_INTERVAL_MS {
                out.push(Problem::error("invalid_value", at, "an interval is at least 1m, so a flow does not run all the time"));
            }
        }
    }
    if let Some(poll) = poll {
        if poll.contains_key("tool") == poll.contains_key("call") {
            out.push(Problem::error("invalid_value", pointer(path, "poll"), "a poll has one of `tool` or `call`: what to run each time"));
        }
    }
    if let Some(file) = map.get("file").and_then(Value::as_object) {
        let globs = file.get("glob").into_iter().chain(file.get("ignore").and_then(Value::as_array).into_iter().flatten());
        for glob in globs.filter_map(Value::as_str) {
            if let Err(error) = glob::Pattern::new(glob) {
                out.push(Problem::error("invalid_value", pointer(path, "file"), format!("`{glob}` is not a file pattern: {error}")));
            }
        }
    }
    let key_field = Field { name: def.key, ty: if def.key == "webhook" { Ty::Open } else { def.ty }, required: true, desc: def.desc };
    let mut also: Vec<&Field> = vec![&key_field];
    also.extend(def.siblings.iter());
    check_fields(map, tables::TRIGGER_COMMON, &also, path, out);
    if def.key == "webhook" {
        if let (Some(inner), Ty::Object(fields)) = (map.get("webhook").and_then(Value::as_object), def.ty) {
            let without_secret: Vec<Field> = fields.iter().map(|f| Field { required: f.required && f.name != "secret", ..*f }).collect();
            check_fields(inner, &without_secret, &[], &pointer(path, "webhook"), out);
        }
    }
}

/// Five fields: minute, hour, day of month, month, day of week
pub fn check_cron(line: &str) -> Result<(), String> {
    crate::trigger::cron::Cron::parse(line).map(|_| ())
}

#[cfg(test)]
mod tests;
