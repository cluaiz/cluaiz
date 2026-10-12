//! Dotted keys over a JSON value (`optimization.n_gpu_layers`, `temp`): used by `cluaiz config` for
//! settings.json and by the chat's `/set parameter` for sampler values.

use serde_json::Value;

/// A value typed on the command line: JSON when it parses (numbers, true/false, null, "quoted"),
/// otherwise the text itself.
pub fn parse_value(raw: &str) -> Value {
    serde_json::from_str(raw.trim()).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Text form of a value: strings without quotes, everything else as compact JSON.
pub fn display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        // Settings hold f32 values: print 0.2, not the f64 widening 0.20000000298023224
        Value::Number(n) if n.is_f64() => n.as_f64().map(|f| (f as f32).to_string()).unwrap_or_else(|| n.to_string()),
        other => other.to_string(),
    }
}

/// Every leaf as (dotted key, value), in the JSON map's key order.
pub fn flatten(value: &Value, prefix: &str, out: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (k, v) in map {
                let key = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
                flatten(v, &key, out);
            }
        }
        leaf => out.push((prefix.to_string(), leaf.clone())),
    }
}

pub fn get<'a>(root: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(root, |cur, part| cur.as_object()?.get(part))
}

/// Replaces the value at `key`; the key must already exist (a typo is an error, never a new field).
pub fn set(root: &mut Value, key: &str, value: Value) -> Result<(), String> {
    let mut cur = root;
    for part in key.split('.') {
        cur = cur
            .as_object_mut()
            .and_then(|m| m.get_mut(part))
            .ok_or_else(|| format!("unknown key `{}`", key))?;
    }
    *cur = value;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_only_existing_keys() {
        let mut v = json!({"a": {"b": 1, "c": null}});
        set(&mut v, "a.c", parse_value("2.5")).unwrap();
        assert_eq!(v["a"]["c"], json!(2.5));
        assert!(set(&mut v, "a.x", json!(1)).is_err());
        assert_eq!(parse_value("hello world"), json!("hello world"));
        assert_eq!(get(&v, "a.b"), Some(&json!(1)));
        let mut out = Vec::new();
        flatten(&v, "", &mut out);
        assert_eq!(out.len(), 2);
    }
}
