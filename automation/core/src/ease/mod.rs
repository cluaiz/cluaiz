//! The easy ways in: the palette of `flows.nodes` (step kinds, triggers, operators), the plain
//! explanation of `flows.explain`, and what `flows.draft` and `flows.fix` give a model and read
//! back. Everything here is made from the format's own tables, so it never lists a field the
//! checker does not know.

pub mod draft;
pub mod explain;

use crate::expr::{Arity, OPS};
use crate::format::tables::{Field, Ty, KINDS, TRIGGERS, TRIGGER_COMMON};
use serde_json::{json, Value};

fn names(fields: &[Field]) -> Vec<&'static str> {
    fields.iter().map(|f| f.name).collect()
}

/// Every step kind with its own fields
pub fn step_kinds() -> Vec<Value> {
    KINDS.iter().map(|k| json!({ "kind": k.key, "fields": names(k.fields), "description": k.desc, "built": k.runs })).collect()
}

/// Every trigger kind with its fields (an object's, and those written beside the kind key), and
/// the fields every trigger may have
pub fn trigger_kinds() -> Vec<Value> {
    TRIGGERS
        .iter()
        .map(|t| {
            let mut fields = match t.ty {
                Ty::Object(fields) => names(fields),
                _ => Vec::new(),
            };
            fields.extend(names(t.siblings));
            json!({ "kind": t.key, "fields": fields, "common": names(TRIGGER_COMMON), "description": t.desc, "built": crate::trigger::BUILT.contains(&t.key) })
        })
        .collect()
}

/// Every expression operator: its group, how many arguments it takes, what it gives
pub fn operators() -> Vec<Value> {
    OPS.iter()
        .map(|o| {
            let arguments = match o.arity {
                Arity::One => json!(1),
                Arity::Range(a, b) if a == b => json!(a),
                Arity::Range(a, b) => json!(format!("{a}-{b}")),
                Arity::AtLeast(n) => json!(format!("{n}+")),
            };
            let returns = if matches!(o.group, "logic" | "comparison") { "bool" } else { "value" };
            json!({ "name": o.name, "group": o.group, "arguments": arguments, "returns": returns })
        })
        .collect()
}
