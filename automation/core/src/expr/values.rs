//! Value rules every operator shares: equality, ordering, numbers, types and times
//! (docs/automation/expressions.md#rules).

use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, Offset, TimeZone, Utc};
use serde_json::{Number, Value};
use std::cmp::Ordering;

/// Whole numbers up to 2^53 are exact in a 64-bit float
const EXACT_INTEGER: f64 = 9_007_199_254_740_992.0;

/// A number as JSON: whole values as integers (`6`, not `6.0`)
pub fn number(value: f64) -> Result<Value, String> {
    if !value.is_finite() {
        return Err("the result is not a finite number".into());
    }
    if value.fract() == 0.0 && value.abs() <= EXACT_INTEGER {
        return Ok(Value::from(value as i64));
    }
    Number::from_f64(value).map(Value::Number).ok_or_else(|| "the result is not a finite number".into())
}

pub fn as_f64(value: &Value) -> Option<f64> {
    value.as_f64()
}

/// The type names of `is` and `type_of`
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0) => "integer",
        Value::Number(_) => "number",
        Value::String(text) if parse_time(text).is_some() => "time",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) if crate::refs::is_file_ref(value) => "file",
        Value::Object(_) => "object",
    }
}

/// Whether `value` is of the type named in `is`
pub fn is_type(value: &Value, name: &str) -> Result<bool, String> {
    let actual = type_name(value);
    Ok(match name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => actual == "integer",
        "bool" => value.is_boolean(),
        "list" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        "time" => actual == "time",
        "file" => actual == "file",
        other => return Err(format!("`{other}` is not a type: use string, number, integer, bool, list, object, null, time or file")),
    })
}

/// `eq`: numbers by value, text exactly, lists item by item, objects field by field
pub fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| equal(a, b)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| equal(v, w)))
        }
        _ => a == b,
    }
}

/// `gt` and friends: both numbers, both times, or both text. `None` when either is `null`.
pub fn compare(a: &Value, b: &Value) -> Result<Option<Ordering>, String> {
    match (a, b) {
        (Value::Null, _) | (_, Value::Null) => Ok(None),
        (Value::Number(x), Value::Number(y)) => Ok(x.as_f64().partial_cmp(&y.as_f64())),
        (Value::String(x), Value::String(y)) => match (parse_time(x), parse_time(y)) {
            (Some(t1), Some(t2)) => Ok(Some(t1.cmp(&t2))),
            _ => Ok(Some(x.as_str().cmp(y.as_str()))),
        },
        _ => Err(format!("cannot compare {} with {}", describe(a), describe(b))),
    }
}

/// "text", "a number", … for error messages
pub fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// RFC 3339 text as a time
pub fn parse_time(text: &str) -> Option<DateTime<FixedOffset>> {
    if text.len() < 20 || !text.as_bytes()[4].eq(&b'-') {
        return None;
    }
    DateTime::parse_from_rfc3339(text).ok()
}

pub fn format_time(time: &DateTime<FixedOffset>) -> String {
    time.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

/// The timezone of day-of-week and time-of-day operators: the flow's, else the computer's
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Zone {
    Local,
    Utc,
    Named(chrono_tz::Tz),
}

impl Zone {
    /// `None` is the computer's zone; `UTC` and IANA names (`Asia/Kolkata`) are read
    pub fn parse(name: Option<&str>) -> Result<Zone, String> {
        match name {
            None => Ok(Zone::Local),
            Some("UTC") | Some("Etc/UTC") => Ok(Zone::Utc),
            Some(name) => name
                .parse::<chrono_tz::Tz>()
                .map(Zone::Named)
                .map_err(|_| format!("`{name}` is not an IANA timezone such as Asia/Kolkata or Europe/Berlin")),
        }
    }

    /// The clock time in this zone
    pub fn naive(&self, time: &DateTime<FixedOffset>) -> NaiveDateTime {
        let utc = time.with_timezone(&Utc);
        match self {
            Zone::Local => utc.with_timezone(&Local).naive_local(),
            Zone::Utc => utc.naive_utc(),
            Zone::Named(tz) => utc.with_timezone(tz).naive_local(),
        }
    }

    /// A clock time in this zone as a time with its offset (the earlier one when ambiguous)
    pub fn from_naive(&self, naive: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
        match self {
            Zone::Local => Local.from_local_datetime(&naive).earliest().map(|t| t.with_timezone(&t.offset().fix())),
            Zone::Utc => Some(Utc.from_utc_datetime(&naive).fixed_offset()),
            Zone::Named(tz) => tz.from_local_datetime(&naive).earliest().map(|t| t.with_timezone(&t.offset().fix())),
        }
    }

    /// Every instant this clock time names: none in a daylight-saving gap, two when the clock goes
    /// back (earlier first)
    pub fn instants(&self, naive: NaiveDateTime) -> Vec<DateTime<Utc>> {
        fn all<T: TimeZone>(result: chrono::LocalResult<DateTime<T>>) -> Vec<DateTime<Utc>> {
            match result {
                chrono::LocalResult::Single(t) => vec![t.with_timezone(&Utc)],
                chrono::LocalResult::Ambiguous(a, b) => vec![a.with_timezone(&Utc), b.with_timezone(&Utc)],
                chrono::LocalResult::None => vec![],
            }
        }
        match self {
            Zone::Local => all(Local.from_local_datetime(&naive)),
            Zone::Utc => vec![Utc.from_utc_datetime(&naive)],
            Zone::Named(tz) => all(tz.from_local_datetime(&naive)),
        }
    }

    /// Now, with this zone's offset
    pub fn now(&self, now: DateTime<Utc>) -> DateTime<FixedOffset> {
        self.from_naive(self.naive(&now.fixed_offset())).unwrap_or_else(|| now.fixed_offset())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn equality_and_order() {
        assert!(equal(&json!(1), &json!(1.0)));
        assert!(!equal(&json!("Delhi"), &json!("delhi")));
        assert!(equal(&json!({"a": [1, 2]}), &json!({"a": [1.0, 2]})));
        assert_eq!(compare(&json!(null), &json!(5)), Ok(None));
        assert!(compare(&json!("10"), &json!(5)).is_err());
        assert_eq!(compare(&json!("2026-11-01T00:00:00Z"), &json!("2026-11-01T05:30:00+05:30")).unwrap(), Some(Ordering::Equal));
    }

    #[test]
    fn numbers_and_types() {
        assert_eq!(number(6.0).unwrap(), json!(6));
        assert_eq!(number(3.5).unwrap(), json!(3.5));
        assert!(number(f64::INFINITY).is_err());
        assert_eq!(type_name(&json!("2026-11-10T18:30:00+05:30")), "time");
        assert_eq!(type_name(&json!({"$file": "a", "bytes": 1})), "file");
        assert_eq!(type_name(&json!(2.0)), "integer");
        assert!(Zone::parse(Some("Asia/Kolkata")).is_ok());
        assert!(Zone::parse(Some("Mars/Base")).is_err());
    }
}
