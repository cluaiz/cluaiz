//! The JSON Schema of the flow file (`flows.schema`), made from the same tables the checker reads,
//! so the schema and the checks cannot drift apart.

use super::tables::{self, Field, Ty};
use serde_json::{json, Map, Value};

pub fn flow_schema() -> Value {
    let mut defs = Map::new();
    defs.insert("step".into(), step_schema());
    defs.insert("steps".into(), json!({ "type": "array", "items": { "$ref": "#/$defs/step" } }));
    defs.insert("trigger".into(), trigger_schema());
    defs.insert(
        "expression".into(),
        json!({ "description": "true, false, or an object with one operator: { \"gt\": [a, b] } (see flows.nodes for every operator)", "anyOf": [{ "type": "boolean" }, { "type": "object", "minProperties": 1, "maxProperties": 1 }] }),
    );
    let mut schema = object(tables::FLOW, &[]);
    let obj = schema.as_object_mut().map(std::mem::take).unwrap_or_default();
    let mut root = Map::new();
    root.insert("$schema".into(), json!("https://json-schema.org/draft/2020-12/schema"));
    root.insert("title".into(), json!("Cluaiz flow"));
    root.extend(obj);
    if let Some(props) = root.get_mut("properties").and_then(Value::as_object_mut) {
        props.insert("flow".into(), json!({ "const": tables::VERSION, "description": "The format version" }));
        if let Some(name) = props.get_mut("name").and_then(Value::as_object_mut) {
            name.insert("pattern".into(), json!("^[a-z0-9][a-z0-9_-]{0,63}$"));
        }
    }
    root.insert("$defs".into(), Value::Object(defs));
    Value::Object(root)
}

fn object(fields: &[Field], also: &[&Field]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for field in fields.iter().chain(also.iter().copied()) {
        let mut schema = of(&field.ty);
        if let Some(map) = schema.as_object_mut() {
            map.insert("description".into(), json!(field.desc));
        }
        properties.insert(field.name.into(), schema);
        if field.required {
            required.push(json!(field.name));
        }
    }
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn of(ty: &Ty) -> Value {
    match ty {
        Ty::Text => json!({ "type": "string" }),
        Ty::Bool => json!({ "type": "boolean" }),
        Ty::Int => json!({ "type": "integer" }),
        Ty::Number => json!({ "type": "number" }),
        Ty::Duration => json!({ "type": "string", "pattern": "^-?([0-9]+(ms|s|m|h|d))+$" }),
        Ty::Size => json!({ "type": "string", "pattern": "^[0-9]+(B|KB|MB|GB)$" }),
        Ty::Time => json!({ "type": "string", "format": "date-time" }),
        Ty::Expr => json!({ "$ref": "#/$defs/expression" }),
        Ty::Any | Ty::Open => json!({}),
        Ty::Enum(values) => json!({ "enum": values }),
        Ty::List(item) => json!({ "type": "array", "items": of(item) }),
        Ty::Object(fields) => object(fields, &[]),
        Ty::Map(item) => json!({ "type": "object", "additionalProperties": of(item) }),
        Ty::Steps => json!({ "$ref": "#/$defs/steps" }),
        Ty::Triggers => json!({ "type": "array", "items": { "$ref": "#/$defs/trigger" } }),
        Ty::Schema => json!({ "type": "object" }),
        Ty::OneOf(options) => json!({ "anyOf": options.iter().map(of).collect::<Vec<_>>() }),
    }
}

/// One branch per kind: the kind key is required, and only its fields and the common ones allowed
fn step_schema() -> Value {
    let branches: Vec<Value> = tables::KINDS
        .iter()
        .map(|kind| {
            let key = Field { name: kind.key, ty: kind.ty, required: true, desc: kind.desc };
            let own: Vec<&Field> = std::iter::once(&key).chain(kind.fields.iter().filter(|f| tables::COMMON.iter().all(|c| c.name != f.name))).collect();
            let mut schema = object(tables::COMMON, &own);
            if let Some(map) = schema.as_object_mut() {
                map.insert("title".into(), json!(kind.key));
            }
            schema
        })
        .collect();
    json!({ "oneOf": branches })
}

fn trigger_schema() -> Value {
    let branches: Vec<Value> = tables::TRIGGERS
        .iter()
        .map(|trigger| {
            let key = Field { name: trigger.key, ty: trigger.ty, required: true, desc: trigger.desc };
            let own: Vec<&Field> = std::iter::once(&key).chain(trigger.siblings.iter()).collect();
            let mut schema = object(tables::TRIGGER_COMMON, &own);
            if let Some(map) = schema.as_object_mut() {
                map.insert("title".into(), json!(trigger.key));
            }
            schema
        })
        .collect();
    json!({ "oneOf": branches })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_every_kind_and_trigger() {
        let schema = flow_schema();
        assert_eq!(schema["properties"]["flow"]["const"], tables::VERSION);
        assert_eq!(schema["required"], json!(["flow", "name", "steps"]));
        assert_eq!(schema["$defs"]["step"]["oneOf"].as_array().map(Vec::len), Some(tables::KINDS.len()));
        assert_eq!(schema["$defs"]["trigger"]["oneOf"].as_array().map(Vec::len), Some(tables::TRIGGERS.len()));
        let ask = &schema["$defs"]["step"]["oneOf"][2];
        assert_eq!(ask["title"], "ask");
        assert!(ask["properties"]["output_schema"].is_object() && ask["properties"]["when"].is_object());
    }
}
