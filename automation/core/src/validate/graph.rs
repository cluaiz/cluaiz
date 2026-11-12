//! Ids, `needs` and references: which step can read which (docs/automation/steps/README.md#ids-and-where-they-can-be-read).

use crate::format::{self, generated_id, inner_lists, step_kind};
use crate::problem::{closest, pointer, Problem};
use crate::{expr, refs};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};

/// Roots a reference may start with everywhere
const ROOTS: [&str; 6] = ["inputs", "trigger", "steps", "vars", "run", "now"];

pub fn check(flow: &Value, out: &mut Vec<Problem>) {
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    format::walk_steps(flow, &mut |step| {
        if step.explicit_id {
            if let Some(first) = ids.get(&step.id) {
                out.push(Problem::error("duplicate_id", pointer(&step.pointer, "id"), format!("`{}` is also the id of the step at {first}", step.id)));
            } else {
                ids.insert(step.id.clone(), step.pointer.clone());
            }
        }
    });
    let all: HashSet<String> = ids.keys().cloned().collect();
    let checker = Checker { all: &all };
    let top = flow.get("steps").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    checker.list(top, None, "steps", "/steps", &HashSet::new(), &[], out);
    // The flow's on_error runs after a failure: any top-level step may be read
    let top_ids: HashSet<String> = top.iter().enumerate().map(|(i, s)| id_of(s, None, "steps", i)).collect();
    if let Some(handlers) = flow.get("on_error").and_then(Value::as_array) {
        checker.list(handlers, None, "on_error", "/on_error", &top_ids, &["error"], out);
    }
    if let Some(outputs) = flow.get("outputs") {
        checker.references(outputs, "/outputs", &top_ids, &[], out);
    }
}

fn id_of(step: &Value, parent: Option<&str>, part: &str, index: usize) -> String {
    step.get("id").and_then(Value::as_str).map(String::from).unwrap_or_else(|| generated_id(parent, part, index))
}

struct Checker<'a> {
    all: &'a HashSet<String>,
}

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    fn list(&self, steps: &[Value], parent: Option<&str>, part: &str, path: &str, outer: &HashSet<String>, locals: &[&str], out: &mut Vec<Problem>) {
        let ids: Vec<String> = steps.iter().enumerate().map(|(i, s)| id_of(s, parent, part, i)).collect();
        let deps: Vec<Vec<usize>> = steps
            .iter()
            .enumerate()
            .map(|(i, step)| match step.get("needs").and_then(Value::as_array) {
                None => if i == 0 { vec![] } else { vec![i - 1] },
                Some(needs) => needs.iter().filter_map(Value::as_str).filter_map(|n| ids.iter().position(|id| id == n)).collect(),
            })
            .collect();
        for (i, step) in steps.iter().enumerate() {
            let here = pointer(path, i);
            for (j, need) in step.get("needs").and_then(Value::as_array).into_iter().flatten().enumerate() {
                let Some(need) = need.as_str() else { continue };
                if !ids.iter().any(|id| id == need) && !outer.contains(need) {
                    let at = pointer(&pointer(&here, "needs"), j);
                    out.push(self.unknown(need, at, "needs names steps of the same list"));
                }
            }
        }
        if let Some(cycle) = find_cycle(&deps) {
            let names: Vec<&str> = cycle.iter().map(|i| ids[*i].as_str()).collect();
            out.push(Problem::error("cycle", pointer(path, cycle[0]), format!("these steps need each other: {}", names.join(" → "))));
            return;
        }
        for (i, step) in steps.iter().enumerate() {
            let Some(map) = step.as_object() else { continue };
            let Ok(kind) = step_kind(map) else { continue };
            let here = pointer(path, i);
            let mut visible = outer.clone();
            for done in before(&deps, i) {
                visible.insert(ids[done].clone());
            }
            let inner = inner_lists(kind, map);
            let inner_fields: HashSet<&str> = inner.iter().map(|l| l.pointer.split('/').nth(1).unwrap_or_default()).collect();
            for (field, value) in map {
                if inner_fields.contains(field.as_str()) && field != "switch" {
                    continue;
                }
                let value = if field == "switch" { strip_case_steps(value) } else { value.clone() };
                let mut field_locals: Vec<&str> = locals.to_vec();
                if field == "output" {
                    field_locals.push("result");
                }
                // `until` is checked after each round: the round's steps and `round` can be read
                let mut field_visible = visible.clone();
                if kind.key == "wait" && field == "wait" {
                    field_locals.push("event");
                }
                if kind.key == "agent" && field == "stop_when" {
                    field_locals.push("round");
                }
                if kind.key == "repeat" && field == "until" {
                    field_locals.push("round");
                    let id = &ids[i];
                    for list in &inner {
                        field_visible.extend(list.steps.iter().enumerate().map(|(n, s)| id_of(s, Some(id), &list.id_part, n)));
                    }
                }
                self.references(&value, &pointer(&here, field), &field_visible, &field_locals, out);
            }
            let mut inner_locals: Vec<&str> = locals.to_vec();
            let as_name = map.get("as").and_then(Value::as_str);
            match kind.key {
                "each" => inner_locals.extend([as_name.unwrap_or("item"), "index", "items"]),
                "repeat" => inner_locals.push("round"),
                _ => {}
            }
            // Inside a round, `steps.<repeat id>.previous` is the round before
            let mut inner_visible = visible.clone();
            if kind.key == "repeat" {
                inner_visible.insert(ids[i].clone());
            }
            for list in &inner {
                let mut list_locals = inner_locals.clone();
                if list.id_part == "on_error" {
                    list_locals = locals.to_vec();
                    list_locals.push("error");
                }
                let seen = if list.id_part == "on_error" { &visible } else { &inner_visible };
                self.list(list.steps, Some(&ids[i]), &list.id_part, &format!("{here}{}", list.pointer), seen, &list_locals, out);
            }
        }
    }

    fn unknown(&self, id: &str, at: String, hint: &str) -> Problem {
        if self.all.contains(id) {
            return Problem::error("not_reachable", at, format!("`{id}` cannot have run before this step ({hint}, or an outer list)"));
        }
        let mut problem = Problem::error("unknown_step", at, format!("there is no step `{id}`"));
        if let Some(close) = closest(id, self.all.iter().map(String::as_str)) {
            problem.message.push_str(&format!("; did you mean `{close}`?"));
            problem = problem.fix(Value::from(close));
        }
        problem
    }

    fn references(&self, value: &Value, at: &str, visible: &HashSet<String>, locals: &[&str], out: &mut Vec<Problem>) {
        let mut paths = Vec::new();
        refs::references_in(value, &mut paths);
        let in_text = paths.len();
        expression_paths(value, &mut paths);
        for (n, path) in paths.into_iter().enumerate() {
            // `item` and `index` inside list operators (`filter`, `map`…) are that operator's own
            let operator_local = n >= in_text;
            let Ok(segments) = refs::parse_path(&path) else { continue };
            let Some(refs::Segment::Field(root)) = segments.first() else { continue };
            if root == "steps" {
                if let Some(refs::Segment::Field(id)) = segments.get(1) {
                    if !visible.contains(id) {
                        let problem = self.unknown(id, at.to_string(), "it is not before this step");
                        out.push(Problem { message: format!("steps.{id}: {}", problem.message), ..problem });
                    }
                }
            } else if !(ROOTS.contains(&root.as_str()) || locals.contains(&root.as_str()) || operator_local && matches!(root.as_str(), "item" | "index")) {
                out.push(Problem::error("invalid_value", at.to_string(), format!("`{root}` cannot be read here; references start with inputs, trigger, steps, vars, run or now")));
            }
        }
    }
}

/// A switch's cases without their `do` lists (those are checked as lists)
fn strip_case_steps(cases: &Value) -> Value {
    match cases {
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|case| match case {
                    Value::Object(map) => Value::Object(map.iter().filter(|(k, _)| *k != "do").map(|(k, v)| (k.clone(), v.clone())).collect::<Map<_, _>>()),
                    other => other.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Paths read by `var`, `exists`, `changed` and `previous` in expressions inside a value
pub fn expression_paths(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| expression_paths(v, out)),
        Value::Object(map) => {
            if let Some((def, args)) = expr::operator_of(value) {
                if matches!(def.name, "var" | "exists" | "changed" | "previous") {
                    let first = match args {
                        Value::Array(items) => items.first(),
                        other => Some(other),
                    };
                    if let Some(Value::String(path)) = first {
                        out.push(path.clone());
                    }
                }
                if def.name != "literal" {
                    expression_paths(args, out);
                }
                return;
            }
            map.values().for_each(|v| expression_paths(v, out));
        }
        _ => {}
    }
}

/// Steps certainly finished before step `i`: its needs, and theirs
fn before(deps: &[Vec<usize>], i: usize) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut stack = deps[i].clone();
    while let Some(next) = stack.pop() {
        if seen.insert(next) {
            stack.extend(deps[next].iter().copied());
        }
    }
    seen
}

fn find_cycle(deps: &[Vec<usize>]) -> Option<Vec<usize>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Open,
        Done,
    }
    fn visit(node: usize, deps: &[Vec<usize>], marks: &mut [Mark], trail: &mut Vec<usize>) -> Option<Vec<usize>> {
        marks[node] = Mark::Open;
        trail.push(node);
        for &next in &deps[node] {
            match marks[next] {
                Mark::Open => {
                    let start = trail.iter().position(|n| *n == next).unwrap_or(0);
                    let mut cycle = trail[start..].to_vec();
                    cycle.push(next);
                    return Some(cycle);
                }
                Mark::New => {
                    if let Some(cycle) = visit(next, deps, marks, trail) {
                        return Some(cycle);
                    }
                }
                Mark::Done => {}
            }
        }
        trail.pop();
        marks[node] = Mark::Done;
        None
    }
    let mut marks = vec![Mark::New; deps.len()];
    (0..deps.len()).find_map(|n| if marks[n] == Mark::New { visit(n, deps, &mut marks, &mut Vec::new()) } else { None })
}
