//! The part of JSON Schema that flows use for `inputs`, `output_schema` and tool inputs: `type`,
//! `enum`, `const`, `properties`, `required`, `additionalProperties`, `items`, `minimum`,
//! `maximum`, `minLength`, `maxLength`, `minItems`, `maxItems`, `pattern`, `default`. Other
//! keywords are accepted and not checked.

use crate::expr::values::{describe, equal};
use serde_json::{json, Map, Value};

const TYPES: [&str; 7] = ["string", "number", "integer", "boolean", "array", "object", "null"];

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaError {
    pub path: String,
    pub message: String,
}

impl SchemaError {
    pub fn to_value(&self) -> Value {
        json!({ "path": self.path, "message": self.message })
    }
}

/// Problems in a schema itself (for `bad_inputs_schema`)
pub fn check_schema(schema: &Value) -> Result<(), String> {
    let Some(map) = schema.as_object() else { return Err("a schema is an object".into()) };
    if let Some(ty) = map.get("type") {
        let names: Vec<&Value> = match ty {
            Value::Array(items) => items.iter().collect(),
            other => vec![other],
        };
        for name in names {
            match name.as_str() {
                Some(t) if TYPES.contains(&t) => {}
                _ => return Err(format!("`type` {name} is not one of {}", TYPES.join(", "))),
            }
        }
    }
    if let Some(pattern) = map.get("pattern") {
        let pattern = pattern.as_str().ok_or("`pattern` must be text")?;
        regex::Regex::new(pattern).map_err(|e| format!("`pattern` does not compile: {e}"))?;
    }
    if let Some(Value::Object(properties)) = map.get("properties") {
        for (name, inner) in properties {
            check_schema(inner).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    if let Some(items) = map.get("items") {
        check_schema(items).map_err(|e| format!("items: {e}"))?;
    }
    Ok(())
}

fn type_fits(value: &Value, name: &str) -> bool {
    match name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|f| f.fract() == 0.0),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// Every place where `value` does not fit `schema`
pub fn validate(value: &Value, schema: &Value) -> Vec<SchemaError> {
    let mut out = Vec::new();
    walk(value, schema, "", &mut out);
    out
}

fn push(out: &mut Vec<SchemaError>, path: &str, message: String) {
    out.push(SchemaError { path: path.to_string(), message });
}

fn walk(value: &Value, schema: &Value, path: &str, out: &mut Vec<SchemaError>) {
    let Some(rules) = schema.as_object() else { return };
    if let Some(ty) = rules.get("type") {
        let names: Vec<&str> = match ty {
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            other => other.as_str().into_iter().collect(),
        };
        if !names.is_empty() && !names.iter().any(|n| type_fits(value, n)) {
            return push(out, path, format!("must be {}, not {}", names.join(" or "), describe(value)));
        }
    }
    if let Some(Value::Array(allowed)) = rules.get("enum") {
        if !allowed.iter().any(|a| equal(a, value)) {
            let list: Vec<String> = allowed.iter().map(Value::to_string).collect();
            push(out, path, format!("must be one of {}", list.join(", ")));
        }
    }
    if let Some(constant) = rules.get("const") {
        if !equal(constant, value) {
            push(out, path, format!("must be {constant}"));
        }
    }
    match value {
        Value::String(text) => {
            let chars = text.chars().count() as u64;
            if rules.get("minLength").and_then(Value::as_u64).is_some_and(|min| chars < min) {
                push(out, path, format!("must have at least {} characters", rules["minLength"]));
            }
            if rules.get("maxLength").and_then(Value::as_u64).is_some_and(|max| chars > max) {
                push(out, path, format!("must have at most {} characters", rules["maxLength"]));
            }
            if let Some(pattern) = rules.get("pattern").and_then(Value::as_str) {
                if regex::Regex::new(pattern).is_ok_and(|re| !re.is_match(text)) {
                    push(out, path, format!("must match the pattern {pattern}"));
                }
            }
        }
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or_default();
            if rules.get("minimum").and_then(Value::as_f64).is_some_and(|min| x < min) {
                push(out, path, format!("must be at least {}", rules["minimum"]));
            }
            if rules.get("maximum").and_then(Value::as_f64).is_some_and(|max| x > max) {
                push(out, path, format!("must be at most {}", rules["maximum"]));
            }
        }
        Value::Array(items) => {
            let count = items.len() as u64;
            if rules.get("minItems").and_then(Value::as_u64).is_some_and(|min| count < min) {
                push(out, path, format!("must have at least {} items", rules["minItems"]));
            }
            if rules.get("maxItems").and_then(Value::as_u64).is_some_and(|max| count > max) {
                push(out, path, format!("must have at most {} items", rules["maxItems"]));
            }
            if let Some(item_schema) = rules.get("items") {
                for (i, item) in items.iter().enumerate() {
                    walk(item, item_schema, &format!("{path}/{i}"), out);
                }
            }
        }
        Value::Object(map) => {
            for name in rules.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                if !map.contains_key(name) {
                    push(out, &format!("{path}/{name}"), "is required".into());
                }
            }
            let properties = rules.get("properties").and_then(Value::as_object);
            for (name, inner) in map {
                match properties.and_then(|p| p.get(name)) {
                    Some(inner_schema) => walk(inner, inner_schema, &format!("{path}/{name}"), out),
                    None => match rules.get("additionalProperties") {
                        Some(Value::Bool(false)) => push(out, &format!("{path}/{name}"), "is not a known field".into()),
                        Some(extra @ Value::Object(_)) => walk(inner, extra, &format!("{path}/{name}"), out),
                        _ => {}
                    },
                }
            }
        }
        _ => {}
    }
}

/// Fills in `default`s of an object schema's properties that are missing
pub fn apply_defaults(value: &mut Map<String, Value>, properties: &Map<String, Value>) {
    for (name, schema) in properties {
        if !value.contains_key(name) {
            if let Some(default) = schema.get("default") {
                value.insert(name.clone(), default.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_the_common_keywords() {
        let schema = json!({ "type": "object", "required": ["kind"], "additionalProperties": false,
            "properties": { "kind": { "enum": ["bug", "feature"] }, "days": { "type": "integer", "minimum": 1, "maximum": 90 } } });
        assert!(validate(&json!({ "kind": "bug", "days": 7 }), &schema).is_empty());
        let errors = validate(&json!({ "kind": "other", "days": 0, "x": 1 }), &schema);
        let paths: Vec<&str> = errors.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["/days", "/kind", "/x"]);
        assert_eq!(validate(&json!({}), &schema)[0].path, "/kind");
        assert!(check_schema(&json!({ "type": "text" })).is_err());
        assert!(check_schema(&json!({ "type": ["string", "null"] })).is_ok());
    }
}
