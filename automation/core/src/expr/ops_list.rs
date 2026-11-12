//! List and object operators that take plain values. The ones that work out a condition per item
//! (`some`, `every`, `filter`, `map`, `find_item`, `count`) are in the evaluator.

use super::values::{compare, describe, equal, number};
use super::{Evaluator, MAX_LIST_ITEMS};
use serde_json::{json, Map, Value};
use std::cmp::Ordering;

type Out = Result<Value, String>;

/// A list argument; `null` counts as empty
fn list(value: &Value) -> Result<Vec<Value>, String> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) if items.len() > MAX_LIST_ITEMS => Err(format!("more than {MAX_LIST_ITEMS} items; use each with batch")),
        Value::Array(items) => Ok(items.clone()),
        other => Err(format!("needs a list, not {}", describe(other))),
    }
}

fn object(value: &Value) -> Result<Map<String, Value>, String> {
    match value {
        Value::Null => Ok(Map::new()),
        Value::Object(map) => Ok(map.clone()),
        other => Err(format!("needs an object, not {}", describe(other))),
    }
}

fn names(value: &Value) -> Result<Vec<String>, String> {
    list(value)?.into_iter().map(|v| v.as_str().map(String::from).ok_or_else(|| "field names must be text".to_string())).collect()
}

fn index(len: usize, at: &Value) -> Result<usize, String> {
    let at = at.as_i64().ok_or("an index must be a whole number")?;
    Ok(if at < 0 { (len as i64 + at).max(0) as usize } else { (at as usize).min(len) })
}

fn numbers(items: &[Value]) -> Result<Vec<f64>, String> {
    items.iter().map(|v| v.as_f64().ok_or_else(|| format!("needs numbers, found {}", describe(v)))).collect()
}

pub fn apply(ev: &mut Evaluator, name: &str, a: &[Value]) -> Out {
    match name {
        "size" => Ok(Value::from(match &a[0] {
            Value::Null => 0,
            Value::String(text) => text.chars().count(),
            Value::Array(items) => items.len(),
            Value::Object(map) => map.len(),
            other => return Err(format!("size needs text, a list or an object, not {}", describe(other))),
        })),
        "in" => Ok(Value::Bool(list(&a[1])?.iter().any(|item| equal(item, &a[0])))),
        "empty" => Ok(Value::Bool(match &a[0] {
            Value::Null => true,
            Value::String(text) => text.is_empty(),
            Value::Array(items) => items.is_empty(),
            Value::Object(map) => map.is_empty(),
            _ => false,
        })),
        "pluck" => {
            let path = a[1].as_str().ok_or("pluck needs a path as text")?;
            Ok(Value::Array(list(&a[0])?.iter().map(|item| ev.path_in(item, path)).collect::<Result<_, _>>()?))
        }
        "first" => Ok(list(&a[0])?.first().cloned().unwrap_or(Value::Null)),
        "last" => Ok(list(&a[0])?.last().cloned().unwrap_or(Value::Null)),
        "slice" => {
            let items = list(&a[0])?;
            let start = index(items.len(), &a[1])?;
            let end = match a.get(2) {
                None | Some(Value::Null) => items.len(),
                Some(end) => index(items.len(), end)?,
            };
            Ok(Value::Array(if start < end { items[start..end].to_vec() } else { Vec::new() }))
        }
        "sort" => sort(ev, a),
        "unique" => {
            let mut kept: Vec<Value> = Vec::new();
            for item in list(&a[0])? {
                if !kept.iter().any(|k| equal(k, &item)) {
                    kept.push(item);
                }
            }
            Ok(Value::Array(kept))
        }
        "flatten" => {
            let mut out = Vec::new();
            for item in list(&a[0])? {
                match item {
                    Value::Array(inner) => out.extend(inner),
                    other => out.push(other),
                }
            }
            Ok(Value::Array(out))
        }
        "sum" => number(numbers(&list(&a[0])?)?.iter().sum()),
        "avg" => {
            let values = numbers(&list(&a[0])?)?;
            if values.is_empty() {
                return Ok(Value::Null);
            }
            number(values.iter().sum::<f64>() / values.len() as f64)
        }
        "list_min" | "list_max" => {
            let items = list(&a[0])?;
            let mut best: Option<Value> = None;
            for item in items {
                best = Some(match best {
                    None => item,
                    Some(current) => {
                        let order = compare(&item, &current)?.unwrap_or(Ordering::Equal);
                        let better = if name == "list_min" { order == Ordering::Less } else { order == Ordering::Greater };
                        if better { item } else { current }
                    }
                });
            }
            Ok(best.unwrap_or(Value::Null))
        }
        "concat_lists" => {
            let mut out = Vec::new();
            for value in a {
                out.extend(list(value)?);
            }
            Ok(Value::Array(out))
        }
        "range" => {
            let start = a[0].as_i64().ok_or("range needs whole numbers")?;
            let end = a[1].as_i64().ok_or("range needs whole numbers")?;
            if end.saturating_sub(start) > 10_000 {
                return Err("range gives at most 10,000 numbers".into());
            }
            Ok(Value::Array((start..end).map(Value::from).collect()))
        }
        "get" => {
            let path = a[1].as_str().ok_or("get needs a path as text")?;
            ev.path_in(&a[0], path)
        }
        "has" => {
            let field = a[1].as_str().ok_or("has needs a field name as text")?;
            Ok(Value::Bool(object(&a[0])?.contains_key(field)))
        }
        "keys" => Ok(Value::Array(object(&a[0])?.keys().map(|k| Value::String(k.clone())).collect())),
        "values" => Ok(Value::Array(object(&a[0])?.values().cloned().collect())),
        "entries" => Ok(Value::Array(object(&a[0])?.into_iter().map(|(key, value)| json!({ "key": key, "value": value })).collect())),
        "from_entries" => {
            let mut out = Map::new();
            for entry in list(&a[0])? {
                let key = entry.get("key").and_then(Value::as_str).ok_or("from_entries needs items { key, value } with text keys")?;
                out.insert(key.to_string(), entry.get("value").cloned().unwrap_or(Value::Null));
            }
            Ok(Value::Object(out))
        }
        "merge" => {
            let mut out = Map::new();
            for value in a {
                out.extend(object(value)?);
            }
            Ok(Value::Object(out))
        }
        "pick" => {
            let wanted = names(&a[1])?;
            Ok(Value::Object(object(&a[0])?.into_iter().filter(|(k, _)| wanted.contains(k)).collect()))
        }
        "omit" => {
            let unwanted = names(&a[1])?;
            Ok(Value::Object(object(&a[0])?.into_iter().filter(|(k, _)| !unwanted.contains(k)).collect()))
        }
        other => Err(format!("`{other}` is not built")),
    }
}

fn sort(ev: &mut Evaluator, a: &[Value]) -> Out {
    let items = list(&a[0])?;
    let path = a.get(1).filter(|v| !v.is_null()).map(|v| v.as_str().map(String::from).ok_or("sort's path must be text")).transpose()?;
    let descending = match a.get(2) {
        None => false,
        Some(Value::String(order)) if order == "desc" => true,
        Some(Value::String(order)) if order == "asc" => false,
        Some(other) => return Err(format!("sort's order is \"asc\" or \"desc\", not {other}")),
    };
    let mut keyed = Vec::with_capacity(items.len());
    for item in items {
        let key = match &path {
            Some(path) => ev.path_in(&item, path)?,
            None => item.clone(),
        };
        keyed.push((key, item));
    }
    let mut problem = None;
    keyed.sort_by(|(x, _), (y, _)| match compare(x, y) {
        Ok(Some(order)) => order,
        // `null` keys go last
        Ok(None) => x.is_null().cmp(&y.is_null()),
        Err(e) => {
            problem.get_or_insert(e);
            Ordering::Equal
        }
    });
    if let Some(problem) = problem {
        return Err(format!("sort: {problem}"));
    }
    if descending {
        keyed.reverse();
    }
    Ok(Value::Array(keyed.into_iter().map(|(_, item)| item).collect()))
}
