//! Expressions: JSON objects with one operator (docs/automation/expressions.md). They only compute:
//! no tools, no files, no network, no endless loops. One operator table ([`OPS`]) is read by the
//! evaluator, by validation and by `flows.schema`.

mod ops_data;
mod ops_list;
mod ops_text;
mod ops_time;
pub mod values;

use crate::refs::{lookup, parse_path, Segment};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashMap;
use values::Zone;

/// Limits of one evaluation (docs/automation/expressions.md#errors-and-limits)
pub const MAX_DEPTH: usize = 64;
pub const MAX_OPERATIONS: u64 = 1_000_000;
pub const MAX_LIST_ITEMS: usize = 100_000;
pub const MAX_TEXT_BYTES: usize = 10 << 20;
pub const MAX_PATTERN_CHARS: usize = 1_000;

/// How many arguments an operator takes. `One` takes its value as it is, even a list
/// (`{ size: [1, 2, 3] }` is 3); the others take a list of arguments (one argument may be written
/// without the list).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arity {
    One,
    Range(usize, usize),
    AtLeast(usize),
}

#[derive(Debug, Clone, Copy)]
pub struct OpDef {
    pub name: &'static str,
    pub group: &'static str,
    pub arity: Arity,
}

const fn op(name: &'static str, group: &'static str, arity: Arity) -> OpDef {
    OpDef { name, group, arity }
}

use Arity::{AtLeast, One, Range};

pub const OPS: &[OpDef] = &[
    op("var", "values", Range(1, 2)),
    op("literal", "values", One),
    op("default", "values", AtLeast(1)),
    op("choose", "values", Range(3, 3)),
    op("all", "logic", AtLeast(0)),
    op("any", "logic", AtLeast(0)),
    op("not", "logic", One),
    op("eq", "comparison", Range(2, 2)),
    op("ne", "comparison", Range(2, 2)),
    op("gt", "comparison", Range(2, 2)),
    op("gte", "comparison", Range(2, 2)),
    op("lt", "comparison", Range(2, 2)),
    op("lte", "comparison", Range(2, 2)),
    op("between", "comparison", Range(3, 3)),
    op("contains", "text", Range(2, 2)),
    op("starts_with", "text", Range(2, 2)),
    op("ends_with", "text", Range(2, 2)),
    op("matches", "text", Range(2, 3)),
    op("find", "text", Range(2, 2)),
    op("lower", "text", One),
    op("upper", "text", One),
    op("trim", "text", One),
    op("replace", "text", Range(3, 3)),
    op("replace_pattern", "text", Range(3, 3)),
    op("split", "text", Range(2, 2)),
    op("join", "text", Range(2, 2)),
    op("concat", "text", AtLeast(0)),
    op("substring", "text", Range(2, 3)),
    op("size", "lists", One),
    op("in", "lists", Range(2, 2)),
    op("empty", "lists", One),
    op("some", "lists", Range(2, 2)),
    op("every", "lists", Range(2, 2)),
    op("filter", "lists", Range(2, 2)),
    op("map", "lists", Range(2, 2)),
    op("pluck", "lists", Range(2, 2)),
    op("find_item", "lists", Range(2, 2)),
    op("first", "lists", One),
    op("last", "lists", One),
    op("slice", "lists", Range(2, 3)),
    op("sort", "lists", Range(1, 3)),
    op("unique", "lists", One),
    op("flatten", "lists", One),
    op("sum", "lists", One),
    op("avg", "lists", One),
    op("list_min", "lists", One),
    op("list_max", "lists", One),
    op("count", "lists", Range(2, 2)),
    op("concat_lists", "lists", AtLeast(0)),
    op("range", "lists", Range(2, 2)),
    op("get", "objects", Range(2, 2)),
    op("has", "objects", Range(2, 2)),
    op("keys", "objects", One),
    op("values", "objects", One),
    op("entries", "objects", One),
    op("from_entries", "objects", One),
    op("merge", "objects", AtLeast(0)),
    op("pick", "objects", Range(2, 2)),
    op("omit", "objects", Range(2, 2)),
    op("object", "objects", One),
    op("exists", "types", One),
    op("is", "types", Range(2, 2)),
    op("type_of", "types", One),
    op("to_number", "types", One),
    op("to_integer", "types", One),
    op("to_text", "types", One),
    op("to_bool", "types", One),
    op("parse_json", "types", One),
    op("to_json", "types", One),
    op("add", "numbers", AtLeast(1)),
    op("sub", "numbers", Range(2, 2)),
    op("mul", "numbers", AtLeast(1)),
    op("div", "numbers", Range(2, 2)),
    op("mod", "numbers", Range(2, 2)),
    op("round", "numbers", Range(1, 2)),
    op("floor", "numbers", One),
    op("ceil", "numbers", One),
    op("abs", "numbers", One),
    op("min", "numbers", AtLeast(1)),
    op("max", "numbers", AtLeast(1)),
    op("before", "time", Range(2, 2)),
    op("after", "time", Range(2, 2)),
    op("within", "time", Range(2, 2)),
    op("weekday", "time", Range(2, 2)),
    op("time_between", "time", Range(3, 3)),
    op("add_time", "time", Range(2, 2)),
    op("diff_time", "time", Range(3, 3)),
    op("start_of", "time", Range(2, 2)),
    op("format_time", "time", Range(2, 2)),
    op("parse_time", "time", Range(2, 2)),
    op("to_time", "time", One),
    op("to_unix_ms", "time", One),
    op("changed", "change", One),
    op("previous", "change", One),
    op("base64_encode", "encoding", One),
    op("base64_decode", "encoding", One),
    op("url_encode", "encoding", One),
    op("sha256", "encoding", One),
];

pub fn op_def(name: &str) -> Option<&'static OpDef> {
    OPS.iter().find(|o| o.name == name)
}

/// The operator of an object with exactly one key that is an operator's name
pub fn operator_of(value: &Value) -> Option<(&'static OpDef, &Value)> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    let (key, args) = map.iter().next()?;
    op_def(key).map(|def| (def, args))
}

/// The arguments as written: a `One` operator's value is its one argument; otherwise a list is the
/// arguments and anything else is one argument
pub fn raw_args<'v>(def: &OpDef, args: &'v Value) -> Vec<&'v Value> {
    match (def.arity, args) {
        (Arity::One, value) => vec![value],
        (_, Value::Array(items)) => items.iter().collect(),
        (_, value) => vec![value],
    }
}

/// Whether the number of arguments fits the operator
pub fn arity_fits(def: &OpDef, count: usize) -> bool {
    match def.arity {
        Arity::One => count == 1,
        Arity::Range(min, max) => (min..=max).contains(&count),
        Arity::AtLeast(min) => count >= min,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExprError {
    /// Where in the expression, such as `all[1].gt`
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// What an evaluation reads: the run's context, the time and zone, and the values the flow's last
/// succeeded run kept for `changed` and `previous`
pub struct Env<'a> {
    pub root: &'a dyn Source,
    pub zone: Zone,
    pub now: DateTime<Utc>,
    pub previous: Option<&'a Map<String, Value>>,
}

/// Where `var` reads paths: a JSON value, or a run's context (which reads large values from files)
pub trait Source {
    fn read(&self, segments: &[Segment]) -> Value;
}

impl Source for Value {
    fn read(&self, segments: &[Segment]) -> Value {
        lookup(self, segments)
    }
}

struct Local {
    item: Value,
    index: usize,
}

pub struct Evaluator<'a> {
    env: &'a Env<'a>,
    locals: Vec<Local>,
    operations: u64,
    patterns: HashMap<(String, bool), Regex>,
    /// Paths read by `changed` / `previous`, with their value now: kept when the run succeeds
    pub watched: Vec<(String, Value)>,
}

type Res = Result<Value, ExprError>;

impl<'a> Evaluator<'a> {
    pub fn new(env: &'a Env<'a>) -> Self {
        Self { env, locals: Vec::new(), operations: 0, patterns: HashMap::new(), watched: Vec::new() }
    }

    /// A value: operators are worked out, lists item by item; other objects are plain values
    pub fn eval(&mut self, value: &Value) -> Res {
        self.eval_at(value, "", 0)
    }

    /// A decision (`when`, `if`, `until`, `filter`): must be `true` or `false`
    pub fn decide(&mut self, value: &Value) -> Result<bool, ExprError> {
        match self.eval(value)? {
            Value::Bool(b) => Ok(b),
            other => Err(ExprError {
                path: String::new(),
                message: format!("a decision needs true or false, not {}; there is no \"truthy\" (use empty, exists or eq)", values::describe(&other)),
            }),
        }
    }

    fn fail(path: &str, message: impl Into<String>) -> ExprError {
        ExprError { path: path.to_string(), message: message.into() }
    }

    fn eval_at(&mut self, value: &Value, path: &str, depth: usize) -> Res {
        if depth > MAX_DEPTH {
            return Err(Self::fail(path, format!("expressions nest at most {MAX_DEPTH} deep")));
        }
        match value {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, item) in items.iter().enumerate() {
                    out.push(self.eval_at(item, &format!("{path}[{i}]"), depth + 1)?);
                }
                Ok(Value::Array(out))
            }
            Value::Object(_) => match operator_of(value) {
                Some((def, args)) => {
                    let here = if path.is_empty() { def.name.to_string() } else { format!("{path}.{}", def.name) };
                    self.count(&here, 1)?;
                    self.apply(def, args, &here, depth)
                }
                None => Ok(value.clone()),
            },
            other => Ok(other.clone()),
        }
    }

    fn count(&mut self, path: &str, n: u64) -> Result<(), ExprError> {
        self.operations += n;
        if self.operations > MAX_OPERATIONS {
            return Err(Self::fail(path, format!("more than {MAX_OPERATIONS} operations in one expression")));
        }
        Ok(())
    }

    fn apply(&mut self, def: &OpDef, args: &Value, path: &str, depth: usize) -> Res {
        let raw = raw_args(def, args);
        if !arity_fits(def, raw.len()) {
            return Err(Self::fail(path, format!("wrong number of arguments ({})", raw.len())));
        }
        // Operators that decide which arguments to work out
        match def.name {
            "literal" => return Ok(args.clone()),
            "var" => return self.var(&raw, path, depth),
            "default" => {
                for (i, arg) in raw.iter().enumerate() {
                    let value = self.eval_at(arg, &format!("{path}[{i}]"), depth + 1)?;
                    if !value.is_null() {
                        return Ok(value);
                    }
                }
                return Ok(Value::Null);
            }
            "choose" => {
                let condition = self.boolean(raw[0], &format!("{path}[0]"), depth)?;
                let pick = if condition { 1 } else { 2 };
                return self.eval_at(raw[pick], &format!("{path}[{pick}]"), depth + 1);
            }
            "all" | "any" => {
                let want = def.name == "any";
                for (i, arg) in raw.iter().enumerate() {
                    if self.boolean(arg, &format!("{path}[{i}]"), depth)? == want {
                        return Ok(Value::Bool(want));
                    }
                }
                return Ok(Value::Bool(!want));
            }
            "object" => {
                let Value::Object(fields) = args else {
                    return Err(Self::fail(path, "object needs { field: expression, … }"));
                };
                let mut out = Map::new();
                for (key, expr) in fields {
                    out.insert(key.clone(), self.eval_at(expr, &format!("{path}.{key}"), depth + 1)?);
                }
                return Ok(Value::Object(out));
            }
            "exists" => {
                let name = self.eval_at(raw[0], path, depth + 1)?;
                let name = name.as_str().ok_or_else(|| Self::fail(path, "exists needs a path as text"))?;
                let found = self.read(name).map_err(|m| Self::fail(path, m))?;
                return Ok(Value::Bool(!found.is_null()));
            }
            "some" | "every" | "filter" | "map" | "find_item" | "count" => {
                let list = self.eval_at(raw[0], &format!("{path}[0]"), depth + 1)?;
                return self.per_item(def.name, list, raw[1], path, depth);
            }
            _ => {}
        }
        let mut values = Vec::with_capacity(raw.len());
        for (i, arg) in raw.iter().enumerate() {
            let at = if raw.len() == 1 { path.to_string() } else { format!("{path}[{i}]") };
            values.push(self.eval_at(arg, &at, depth + 1)?);
        }
        let result = match def.group {
            "text" => ops_text::apply(self, def.name, &values),
            "lists" | "objects" => ops_list::apply(self, def.name, &values),
            "time" => ops_time::apply(self.env, def.name, &values),
            "change" => self.change(def.name, &values),
            _ => ops_data::apply(def.name, &values),
        };
        let value = result.map_err(|message| Self::fail(path, message))?;
        if let Value::String(text) = &value {
            if text.len() > MAX_TEXT_BYTES {
                return Err(Self::fail(path, "the text result is larger than 10 MB"));
            }
        }
        Ok(value)
    }

    fn boolean(&mut self, value: &Value, path: &str, depth: usize) -> Result<bool, ExprError> {
        match self.eval_at(value, path, depth + 1)? {
            Value::Bool(b) => Ok(b),
            other => Err(Self::fail(path, format!("needs true or false, not {}", values::describe(&other)))),
        }
    }

    fn var(&mut self, raw: &[&Value], path: &str, depth: usize) -> Res {
        let name = self.eval_at(raw[0], path, depth + 1)?;
        let Some(name) = name.as_str() else {
            return Err(Self::fail(path, "var needs a path as text"));
        };
        let found = self.read(name).map_err(|m| Self::fail(path, m))?;
        if found.is_null() && raw.len() == 2 {
            return self.eval_at(raw[1], &format!("{path}[1]"), depth + 1);
        }
        Ok(found)
    }

    /// A path: `item` and `index` inside list operators, else the run's context
    pub fn read(&self, path: &str) -> Result<Value, String> {
        let segments = parse_path(path)?;
        if let (Some(local), Some(Segment::Field(first))) = (self.locals.last(), segments.first()) {
            match first.as_str() {
                "item" => return Ok(lookup(&local.item, &segments[1..])),
                "index" if segments.len() == 1 => return Ok(Value::from(local.index)),
                _ => {}
            }
        }
        if segments == [Segment::Field("now".into())] {
            return Ok(Value::String(values::format_time(&self.env.zone.now(self.env.now))));
        }
        Ok(self.env.root.read(&segments))
    }

    fn per_item(&mut self, name: &str, list: Value, expr: &Value, path: &str, depth: usize) -> Res {
        let items = match list {
            Value::Null => Vec::new(),
            Value::Array(items) => items,
            other => return Err(Self::fail(path, format!("{name} needs a list, not {}", values::describe(&other)))),
        };
        if items.len() > MAX_LIST_ITEMS {
            return Err(Self::fail(path, format!("more than {MAX_LIST_ITEMS} items; use each with batch")));
        }
        self.count(path, items.len() as u64)?;
        let at = format!("{path}[1]");
        let mut kept = Vec::new();
        let mut hits = 0usize;
        for (index, item) in items.into_iter().enumerate() {
            self.locals.push(Local { item: item.clone(), index });
            let outcome = if name == "map" {
                self.eval_at(expr, &at, depth + 1).map(Some)
            } else {
                self.boolean(expr, &at, depth).map(|b| b.then_some(Value::Null))
            };
            self.locals.pop();
            let outcome = outcome?;
            match (name, outcome) {
                ("map", Some(value)) => kept.push(value),
                ("some", Some(_)) => return Ok(Value::Bool(true)),
                ("every", None) => return Ok(Value::Bool(false)),
                ("find_item", Some(_)) => return Ok(item),
                ("filter", Some(_)) => kept.push(item),
                ("count", Some(_)) => hits += 1,
                _ => {}
            }
        }
        Ok(match name {
            "some" => Value::Bool(false),
            "every" => Value::Bool(true),
            "find_item" => Value::Null,
            "count" => Value::from(hits),
            _ => Value::Array(kept),
        })
    }

    fn change(&mut self, name: &str, values: &[Value]) -> Result<Value, String> {
        let path = values[0].as_str().ok_or("needs a path as text")?.to_string();
        let now = self.read(&path)?;
        let before = self.env.previous.and_then(|p| p.get(&path)).cloned();
        self.watched.push((path, now.clone()));
        Ok(match name {
            "changed" => Value::Bool(before.is_none_or(|b| !values::equal(&b, &now))),
            _ => before.unwrap_or(Value::Null),
        })
    }

    /// A compiled pattern (linear time, at most 1,000 characters), kept for the evaluation
    pub(crate) fn pattern(&mut self, pattern: &str, ignore_case: bool) -> Result<Regex, String> {
        if pattern.chars().count() > MAX_PATTERN_CHARS {
            return Err(format!("a pattern is at most {MAX_PATTERN_CHARS} characters"));
        }
        let key = (pattern.to_string(), ignore_case);
        if let Some(found) = self.patterns.get(&key) {
            return Ok(found.clone());
        }
        let compiled = regex::RegexBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .size_limit(1 << 20)
            .build()
            .map_err(|e| format!("the pattern does not compile: {e}"))?;
        self.patterns.insert(key, compiled.clone());
        Ok(compiled)
    }

    /// The `item` inside `sort` and `pluck` paths
    pub(crate) fn path_in(&self, item: &Value, path: &str) -> Result<Value, String> {
        Ok(lookup(item, &parse_path(path)?))
    }
}

/// Checks the shape of an expression before a run: operator names and argument counts. Calls
/// `report(path, message)` for each problem.
pub fn check_shape(value: &Value, path: &str, report: &mut dyn FnMut(&str, &'static str, String)) {
    match value {
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                check_shape(item, &format!("{path}/{i}"), report);
            }
        }
        Value::Object(map) => {
            let Some((def, args)) = operator_of(value) else {
                if map.len() == 1 {
                    let key = map.keys().next().map(String::as_str).unwrap_or_default();
                    if let Some(close) = close_operator(key) {
                        report(path, "unknown_operator", format!("`{key}` is not an operator; did you mean `{close}`?"));
                    }
                }
                return;
            };
            let here = format!("{path}/{}", def.name);
            if def.name == "literal" {
                return;
            }
            if def.name == "object" {
                match args {
                    Value::Object(fields) => fields.iter().for_each(|(k, v)| check_shape(v, &format!("{here}/{k}"), report)),
                    _ => report(&here, "bad_arguments", "object needs { field: expression, … }".into()),
                }
                return;
            }
            let raw = raw_args(def, args);
            if !arity_fits(def, raw.len()) {
                report(&here, "bad_arguments", format!("`{}` does not take {} arguments", def.name, raw.len()));
            }
            if matches!(def.name, "matches" | "find" | "replace_pattern") {
                if let Some(Value::String(pattern)) = raw.get(1) {
                    let too_long = pattern.chars().count() > MAX_PATTERN_CHARS;
                    if too_long || regex::RegexBuilder::new(pattern).size_limit(1 << 20).build().is_err() {
                        report(&format!("{here}/1"), "bad_pattern", format!("the pattern `{pattern}` does not compile or is over {MAX_PATTERN_CHARS} characters"));
                    }
                }
            }
            if def.arity == Arity::One {
                check_shape(args, &here, report);
            } else if let Value::Array(items) = args {
                for (i, item) in items.iter().enumerate() {
                    check_shape(item, &format!("{here}/{i}"), report);
                }
            } else {
                check_shape(args, &here, report);
            }
        }
        _ => {}
    }
}

/// An operator name one or two edits away
pub fn close_operator(name: &str) -> Option<&'static str> {
    OPS.iter()
        .map(|o| (strsim::levenshtein(name, o.name), o.name))
        .filter(|(d, _)| *d > 0 && *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}

#[cfg(test)]
mod tests;
