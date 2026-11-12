//! Text operators. `null` text counts as `""`; numbers and booleans must be converted with
//! `to_text`. Patterns run in linear time (no look-around, no back-references).

use super::ops_data::text;
use super::Evaluator;
use crate::refs::to_text;
use serde_json::{json, Value};

type Out = Result<Value, String>;

pub fn apply(ev: &mut Evaluator, name: &str, a: &[Value]) -> Out {
    match name {
        "contains" => Ok(Value::Bool(text(&a[0])?.contains(&text(&a[1])?))),
        "starts_with" => Ok(Value::Bool(text(&a[0])?.starts_with(&text(&a[1])?))),
        "ends_with" => Ok(Value::Bool(text(&a[0])?.ends_with(&text(&a[1])?))),
        "matches" => {
            let ignore_case = match a.get(2) {
                None => false,
                Some(Value::String(flag)) if flag == "i" => true,
                Some(other) => return Err(format!("the third argument of matches is \"i\", not {other}")),
            };
            let pattern = ev.pattern(&text(&a[1])?, ignore_case)?;
            Ok(Value::Bool(pattern.is_match(&text(&a[0])?)))
        }
        "find" => {
            let pattern = ev.pattern(&text(&a[1])?, false)?;
            let haystack = text(&a[0])?;
            Ok(match pattern.captures(&haystack) {
                None => Value::Null,
                Some(caps) => {
                    let groups: Vec<Value> = caps.iter().skip(1).map(|g| g.map(|m| Value::String(m.as_str().into())).unwrap_or(Value::Null)).collect();
                    json!({ "match": caps.get(0).map(|m| m.as_str()).unwrap_or_default(), "groups": groups })
                }
            })
        }
        "lower" => Ok(Value::String(text(&a[0])?.to_lowercase())),
        "upper" => Ok(Value::String(text(&a[0])?.to_uppercase())),
        "trim" => Ok(Value::String(text(&a[0])?.trim().to_string())),
        "replace" => {
            let from = text(&a[1])?;
            if from.is_empty() {
                return Err("replace needs text to find".into());
            }
            Ok(Value::String(text(&a[0])?.replace(&from, &text(&a[2])?)))
        }
        "replace_pattern" => {
            let pattern = ev.pattern(&text(&a[1])?, false)?;
            Ok(Value::String(pattern.replace_all(&text(&a[0])?, text(&a[2])?.as_str()).into_owned()))
        }
        "split" => {
            let separator = text(&a[1])?;
            let whole = text(&a[0])?;
            if separator.is_empty() {
                return Ok(Value::Array(whole.chars().map(|c| Value::String(c.to_string())).collect()));
            }
            Ok(Value::Array(whole.split(separator.as_str()).map(|p| Value::String(p.into())).collect()))
        }
        "join" => {
            let items = match &a[0] {
                Value::Null => Vec::new(),
                Value::Array(items) => items.clone(),
                other => return Err(format!("join needs a list, not {}", super::values::describe(other))),
            };
            let parts: Vec<String> = items.iter().map(to_text).collect();
            Ok(Value::String(parts.join(&text(&a[1])?)))
        }
        "concat" => Ok(Value::String(a.iter().map(to_text).collect())),
        "substring" => {
            let chars: Vec<char> = text(&a[0])?.chars().collect();
            let start = a[1].as_i64().ok_or("substring's start must be a whole number")?;
            let start = if start < 0 { (chars.len() as i64 + start).max(0) as usize } else { (start as usize).min(chars.len()) };
            let end = match a.get(2) {
                None | Some(Value::Null) => chars.len(),
                Some(length) => {
                    let length = length.as_i64().filter(|l| *l >= 0).ok_or("substring's length must be a whole number, 0 or more")?;
                    (start + length as usize).min(chars.len())
                }
            };
            Ok(Value::String(chars[start..end].iter().collect()))
        }
        other => Err(format!("`{other}` is not built")),
    }
}
