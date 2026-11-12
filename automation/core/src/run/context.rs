//! The run's context: what references and expressions read (docs/automation/references.md#the-context).
//! Steps are kept per list, so a step inside a branch or a loop item is seen only there.

use crate::error::{FlowError, Kind};
use crate::expr::values::{format_time, Zone};
use crate::expr::{Env, Evaluator, Source};
use crate::refs::{self, Piece, Segment};
use chrono::Utc;
use serde_json::{Map, Value};
use std::path::PathBuf;

#[derive(Clone)]
pub struct Context {
    pub inputs: Value,
    pub trigger: Value,
    pub run: Value,
    /// `vars`: an object of the values `set` steps kept
    pub vars: Value,
    /// `steps.<id>` records, one map per open list, innermost last
    scopes: Vec<Map<String, Value>>,
    /// `item`, `index`, `items`, `round`, `error`, `result`, innermost last
    locals: Vec<(String, Value)>,
    pub zone: Zone,
    /// Values the last succeeded run kept for `changed` and `previous`
    pub previous: Map<String, Value>,
    /// Paths read by `changed` / `previous` in this run, kept when it succeeds
    pub watched: Vec<(String, Value)>,
    /// The flows folder: file references are relative to it
    pub flows_dir: PathBuf,
}

impl Context {
    pub fn new(inputs: Value, trigger: Value, run: Value, zone: Zone, previous: Map<String, Value>, flows_dir: PathBuf) -> Self {
        Self { inputs, trigger, run, vars: Value::Object(Map::new()), scopes: vec![Map::new()], locals: Vec::new(), zone, previous, watched: Vec::new(), flows_dir }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Map::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Records a finished step for the steps after it in the same list (or deeper)
    pub fn set_step(&mut self, id: &str, record: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(id.to_string(), record);
        }
    }

    /// A step's record in the innermost list
    pub fn step_view(&self, id: &str) -> Option<Value> {
        self.scopes.last().and_then(|scope| scope.get(id)).cloned()
    }

    pub fn push_local(&mut self, name: &str, value: Value) {
        self.locals.push((name.to_string(), value));
    }

    pub fn pop_locals(&mut self, count: usize) {
        let keep = self.locals.len().saturating_sub(count);
        self.locals.truncate(keep);
    }

    /// The file a file reference names
    pub fn file_path(&self, reference: &Value) -> Option<PathBuf> {
        reference.get("$file").and_then(Value::as_str).map(|relative| self.flows_dir.join(relative))
    }

    /// Reads a JSON file reference, to go on into it with a path
    fn open_file(&self, reference: &Value) -> Option<Value> {
        let path = self.file_path(reference)?;
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    /// Like `lookup`, but a path that goes on into a file reference reads that part of the file
    fn walk(&self, start: &Value, segments: &[Segment]) -> Value {
        let mut current = start;
        for (i, segment) in segments.iter().enumerate() {
            if refs::is_file_ref(current) {
                return match self.open_file(current) {
                    Some(content) => self.walk(&content, &segments[i..]),
                    None => Value::Null,
                };
            }
            let next = match (segment, current) {
                (Segment::Field(name), Value::Object(map)) => map.get(name),
                (Segment::Index(index), Value::Array(items)) => {
                    let at = if *index < 0 { items.len() as i64 + index } else { *index };
                    usize::try_from(at).ok().and_then(|at| items.get(at))
                }
                _ => None,
            };
            match next {
                Some(next) => current = next,
                None => return Value::Null,
            }
        }
        current.clone()
    }

    pub fn read_path(&self, segments: &[Segment]) -> Value {
        let Some(Segment::Field(root)) = segments.first() else { return Value::Null };
        let rest = &segments[1..];
        if let Some((_, value)) = self.locals.iter().rev().find(|(name, _)| name == root) {
            return self.walk(value, rest);
        }
        match root.as_str() {
            "steps" => {
                let Some(Segment::Field(id)) = rest.first() else { return Value::Null };
                self.scopes.iter().rev().find_map(|scope| scope.get(id)).map(|record| self.walk(record, &rest[1..])).unwrap_or(Value::Null)
            }
            "inputs" => self.walk(&self.inputs, rest),
            "trigger" => self.walk(&self.trigger, rest),
            "run" => self.walk(&self.run, rest),
            "vars" => self.walk(&self.vars, rest),
            "now" if rest.is_empty() => Value::String(format_time(&self.zone.now(Utc::now()))),
            _ => Value::Null,
        }
    }

    fn evaluate(&mut self, expression: &Value, decide: bool) -> Result<Value, FlowError> {
        let previous = std::mem::take(&mut self.previous);
        let outcome = {
            let env = Env { root: &*self, zone: self.zone, now: Utc::now(), previous: Some(&previous) };
            let mut evaluator = Evaluator::new(&env);
            let value = if decide { evaluator.decide(expression).map(Value::Bool) } else { evaluator.eval(expression) };
            (value, std::mem::take(&mut evaluator.watched))
        };
        self.previous = previous;
        let (value, watched) = outcome;
        self.watched.extend(watched);
        value.map_err(|e| FlowError::new(Kind::Expression, e.to_string()))
    }

    /// A decision: `when`, `if`, a case's `when`
    pub fn decide(&mut self, expression: &Value) -> Result<bool, FlowError> {
        Ok(self.evaluate(expression, true)? == Value::Bool(true))
    }

    /// A value with references and `{ expr: … }` worked out
    pub fn resolve(&mut self, value: &Value) -> Result<Value, FlowError> {
        match value {
            Value::String(text) if text.contains("${{") => self.template(text),
            Value::Array(items) => items.iter().map(|v| self.resolve(v)).collect::<Result<_, _>>().map(Value::Array),
            Value::Object(map) if map.len() == 1 && map.contains_key("expr") => self.evaluate(&map["expr"], false),
            Value::Object(map) => {
                let mut out = Map::new();
                for (key, inner) in map {
                    out.insert(key.clone(), self.resolve(inner)?);
                }
                Ok(Value::Object(out))
            }
            other => Ok(other.clone()),
        }
    }

    /// A string: a string that is one reference keeps the value's type; others become text
    pub fn template(&mut self, text: &str) -> Result<Value, FlowError> {
        let pieces = refs::split_template(text).map_err(|m| FlowError::new(Kind::Expression, m))?;
        let read = |path: &str| -> Result<Value, FlowError> {
            let segments = refs::parse_path(path).map_err(|m| FlowError::new(Kind::Expression, m))?;
            Ok(self.read_path(&segments))
        };
        if let [Piece::Ref(path)] = pieces.as_slice() {
            return read(path);
        }
        let mut out = String::new();
        for piece in &pieces {
            match piece {
                Piece::Text(plain) => out.push_str(plain),
                Piece::Ref(path) => out.push_str(&refs::to_text(&read(path)?)),
            }
        }
        Ok(Value::String(out))
    }
}

impl Source for Context {
    fn read(&self, segments: &[Segment]) -> Value {
        self.read_path(segments)
    }
}
