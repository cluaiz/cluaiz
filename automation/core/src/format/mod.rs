//! The flow format: the tables that describe it, the parser, the structure check and the JSON
//! Schema. Also how steps are found inside a flow: their kind, ids, places and inner lists, used
//! by validation and by the runner alike.

pub mod check;
pub mod json_schema;
pub mod parse;
pub mod tables;

use serde_json::{Map, Value};
use tables::KindDef;

pub use parse::{parse_text, Format};

pub enum KindError {
    None,
    Many(Vec<&'static str>),
}

/// A step's kind: the one kind key it has. A key that is also a field of the found kind (`parallel`
/// of `each`, `wait` of `run`) is that field, not a second kind.
pub fn step_kind(map: &Map<String, Value>) -> Result<&'static KindDef, KindError> {
    let present: Vec<&'static KindDef> = tables::KINDS.iter().filter(|k| map.contains_key(k.key)).collect();
    match present.as_slice() {
        [] => Err(KindError::None),
        [one] => Ok(one),
        many => {
            let fits: Vec<&&'static KindDef> = many
                .iter()
                .filter(|candidate| many.iter().all(|other| other.key == candidate.key || tables::kind_field(candidate, other.key).is_some()))
                .collect();
            match fits.as_slice() {
                [one] => Ok(one),
                _ => Err(KindError::Many(many.iter().map(|k| k.key).collect())),
            }
        }
    }
}

/// A list of steps inside a step (`then`, a case's `do`, a branch)
pub struct InnerList<'a> {
    /// For generated ids: `then`, `switch.0.do`
    pub id_part: String,
    /// For records: `then`, `switch[0].do`
    pub place_part: String,
    /// JSON Pointer from the step: `/then`, `/switch/0/do`
    pub pointer: String,
    pub steps: &'a [Value],
}

fn list<'a>(map: &'a Map<String, Value>, field: &str) -> Option<&'a [Value]> {
    map.get(field).and_then(Value::as_array).map(Vec::as_slice)
}

/// The step lists a step holds, in file order (the step's own `on_error` list last)
pub fn inner_lists<'a>(kind: &KindDef, map: &'a Map<String, Value>) -> Vec<InnerList<'a>> {
    let simple = |field: &str| list(map, field).map(|steps| InnerList { id_part: field.into(), place_part: field.into(), pointer: format!("/{field}"), steps });
    let mut out: Vec<InnerList> = Vec::new();
    match kind.key {
        "if" => out.extend(["then", "else"].into_iter().filter_map(simple)),
        "switch" => {
            if let Some(cases) = list(map, "switch") {
                for (i, case) in cases.iter().enumerate() {
                    if let Some(steps) = case.get("do").and_then(Value::as_array) {
                        out.push(InnerList { id_part: format!("switch.{i}.do"), place_part: format!("switch[{i}].do"), pointer: format!("/switch/{i}/do"), steps });
                    }
                }
            }
            out.extend(simple("default"));
        }
        "each" => out.extend(simple("do")),
        "repeat" => out.extend(simple("repeat")),
        "parallel" => {
            if let Some(branches) = map.get("parallel").and_then(Value::as_object) {
                for (name, steps) in branches {
                    if let Some(steps) = steps.as_array() {
                        out.push(InnerList { id_part: format!("parallel.{name}"), place_part: format!("parallel.{name}"), pointer: format!("/parallel/{name}"), steps });
                    }
                }
            }
        }
        _ => {}
    }
    out.extend(simple("on_error"));
    out
}

/// One step as validation and permissions see it
pub struct StepRef<'a> {
    pub map: &'a Map<String, Value>,
    pub kind: &'static KindDef,
    /// The `id`, or one made from its place (`_2`, `_2.then.0`)
    pub id: String,
    pub explicit_id: bool,
    pub pointer: String,
    pub place: String,
    /// Inside `each`, `repeat`, `on_error` lists…: the kinds of the steps around it, outermost first
    pub around: Vec<&'static str>,
    /// The last step of its list: its output is the list's output
    pub last: bool,
}

/// The id a step gets from its place when it has none
pub fn generated_id(parent: Option<&str>, part: &str, index: usize) -> String {
    match parent {
        None if part == "steps" => format!("_{index}"),
        None => format!("_{part}.{index}"),
        Some(parent) => format!("{parent}.{part}.{index}"),
    }
}

/// Every step of the flow (`steps`, the flow's `on_error`, and every inner list), depth first
pub fn walk_steps<'a>(flow: &'a Value, visit: &mut dyn FnMut(&StepRef<'a>)) {
    for top in ["steps", "on_error"] {
        if let Some(steps) = flow.get(top).and_then(Value::as_array) {
            walk_list(steps, None, top, &format!("/{top}"), top, &[], visit);
        }
    }
}

fn walk_list<'a>(steps: &'a [Value], parent: Option<&str>, id_part: &str, pointer: &str, place: &str, around: &[&'static str], visit: &mut dyn FnMut(&StepRef<'a>)) {
    for (i, step) in steps.iter().enumerate() {
        let Some(map) = step.as_object() else { continue };
        let Ok(kind) = step_kind(map) else { continue };
        let explicit = map.get("id").and_then(Value::as_str).map(String::from);
        let id = explicit.clone().unwrap_or_else(|| generated_id(parent, id_part, i));
        let here = StepRef { map, kind, id: id.clone(), explicit_id: explicit.is_some(), pointer: format!("{pointer}/{i}"), place: format!("{place}[{i}]"), around: around.to_vec(), last: i + 1 == steps.len() };
        visit(&here);
        let mut inner_around = around.to_vec();
        inner_around.push(kind.key);
        for inner in inner_lists(kind, map) {
            walk_list(inner.steps, Some(&id), &inner.id_part, &format!("{}{}", here.pointer, inner.pointer), &format!("{}.{}", here.place, inner.place_part), &inner_around, visit);
        }
    }
}
