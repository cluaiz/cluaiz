//! Logic (`not`), comparison, types and conversion, numbers, encoding.

use super::values::{self, compare, describe, equal, number};
use base64::Engine as _;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

type Out = Result<Value, String>;

pub fn apply(name: &str, a: &[Value]) -> Out {
    match name {
        "not" => match &a[0] {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            other => Err(format!("not needs true or false, not {}", describe(other))),
        },
        "eq" => Ok(Value::Bool(equal(&a[0], &a[1]))),
        "ne" => Ok(Value::Bool(!equal(&a[0], &a[1]))),
        "gt" => ordered(&a[0], &a[1], |o| o == Ordering::Greater),
        "gte" => ordered(&a[0], &a[1], |o| o != Ordering::Less),
        "lt" => ordered(&a[0], &a[1], |o| o == Ordering::Less),
        "lte" => ordered(&a[0], &a[1], |o| o != Ordering::Greater),
        "between" => {
            let low = ordered(&a[0], &a[1], |o| o != Ordering::Less)?;
            let high = ordered(&a[0], &a[2], |o| o != Ordering::Greater)?;
            Ok(Value::Bool(low == Value::Bool(true) && high == Value::Bool(true)))
        }
        "exists" => Err("exists is worked out with the run's context".into()),
        "is" => {
            let type_name = a[1].as_str().ok_or("is needs a type name as text")?;
            values::is_type(&a[0], type_name).map(Value::Bool)
        }
        "type_of" => Ok(Value::String(values::type_name(&a[0]).into())),
        "to_number" => to_number(&a[0]),
        "to_integer" => match to_number(&a[0])? {
            Value::Null => Ok(Value::Null),
            n => number(n.as_f64().unwrap_or_default().trunc()),
        },
        "to_text" => Ok(Value::String(crate::refs::to_text(&a[0]))),
        "to_bool" => to_bool(&a[0]),
        "parse_json" => {
            let text = a[0].as_str().ok_or("parse_json needs text")?;
            serde_json::from_str(text).map_err(|e| format!("the text is not JSON: {e}"))
        }
        "to_json" => Ok(Value::String(a[0].to_string())),
        "add" => fold(a, 0.0, |x, y| x + y),
        "mul" => fold(a, 1.0, |x, y| x * y),
        "sub" => binary(a, |x, y| Ok(x - y)),
        "div" => binary(a, |x, y| if y == 0.0 { Err("dividing by 0".into()) } else { Ok(x / y) }),
        "mod" => binary(a, |x, y| if y == 0.0 { Err("dividing by 0".into()) } else { Ok(x % y) }),
        "round" => {
            let Some(x) = nullable_number(&a[0])? else { return Ok(Value::Null) };
            let digits = a.get(1).map(|d| d.as_i64().ok_or("round's digits must be a whole number")).transpose()?.unwrap_or(0);
            let scale = 10f64.powi(digits.clamp(-15, 15) as i32);
            number((x * scale).round() / scale)
        }
        "floor" => unary(&a[0], f64::floor),
        "ceil" => unary(&a[0], f64::ceil),
        "abs" => unary(&a[0], f64::abs),
        "min" | "max" => {
            let mut best: Option<f64> = None;
            for value in a {
                let Some(x) = nullable_number(value)? else { return Ok(Value::Null) };
                best = Some(match best {
                    None => x,
                    Some(b) if name == "min" => b.min(x),
                    Some(b) => b.max(x),
                });
            }
            best.map(number).unwrap_or(Ok(Value::Null))
        }
        "base64_encode" => Ok(Value::String(base64::engine::general_purpose::STANDARD.encode(text(&a[0])?))),
        "base64_decode" => {
            let bytes = base64::engine::general_purpose::STANDARD.decode(text(&a[0])?).map_err(|e| format!("not Base64: {e}"))?;
            String::from_utf8(bytes).map(Value::String).map_err(|_| "the decoded bytes are not UTF-8 text".into())
        }
        "url_encode" => Ok(Value::String(url_encode(&text(&a[0])?))),
        "sha256" => {
            let digest = Sha256::digest(text(&a[0])?.as_bytes());
            Ok(Value::String(digest.iter().map(|b| format!("{b:02x}")).collect()))
        }
        other => Err(format!("`{other}` is not built")),
    }
}

fn ordered(a: &Value, b: &Value, test: impl Fn(Ordering) -> bool) -> Out {
    Ok(Value::Bool(compare(a, b)?.is_some_and(test)))
}

/// `null` stays `null` in number functions; other types are an error
fn nullable_number(value: &Value) -> Result<Option<f64>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(n.as_f64()),
        other => Err(format!("needs a number, not {}; convert with to_number", describe(other))),
    }
}

fn unary(value: &Value, f: fn(f64) -> f64) -> Out {
    match nullable_number(value)? {
        None => Ok(Value::Null),
        Some(x) => number(f(x)),
    }
}

fn binary(a: &[Value], f: impl Fn(f64, f64) -> Result<f64, String>) -> Out {
    match (nullable_number(&a[0])?, nullable_number(&a[1])?) {
        (Some(x), Some(y)) => number(f(x, y)?),
        _ => Ok(Value::Null),
    }
}

fn fold(a: &[Value], start: f64, f: fn(f64, f64) -> f64) -> Out {
    let mut total = start;
    for value in a {
        match nullable_number(value)? {
            None => return Ok(Value::Null),
            Some(x) => total = f(total, x),
        }
    }
    number(total)
}

fn to_number(value: &Value) -> Out {
    match value {
        Value::Null => Ok(Value::Null),
        Value::Number(_) => Ok(value.clone()),
        Value::Bool(b) => Ok(Value::from(u8::from(*b))),
        Value::String(text) => {
            let parsed: f64 = text.trim().parse().map_err(|_| format!("\"{text}\" is not a number"))?;
            number(parsed)
        }
        other => Err(format!("to_number cannot read {}", describe(other))),
    }
}

fn to_bool(value: &Value) -> Out {
    let truthy = match value {
        Value::Bool(b) => Some(*b),
        Value::Null => Some(false),
        Value::Number(n) if n.as_f64() == Some(1.0) => Some(true),
        Value::Number(n) if n.as_f64() == Some(0.0) => Some(false),
        Value::String(text) => match text.as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" | "" => Some(false),
            _ => None,
        },
        _ => None,
    };
    truthy.map(Value::Bool).ok_or_else(|| format!("to_bool cannot read {value}"))
}

/// Text arguments: `null` counts as empty; numbers and booleans must be converted on purpose
pub(super) fn text(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(text) => Ok(text.clone()),
        other => Err(format!("needs text, not {}; convert with to_text", describe(other))),
    }
}

fn url_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
