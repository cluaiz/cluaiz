//! Run limits and step timeouts (docs/automation/limits.md). Every default is *proposed*.

use crate::units::{parse_duration, parse_size};
use serde_json::{json, Value};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// The defaults `normalized` shows
pub fn defaults() -> Vec<(&'static str, Value)> {
    vec![("timeout", json!("1h")), ("max_wait", json!("7d")), ("max_steps", json!(10_000)), ("max_parallel", json!(4)), ("result_in_memory", json!("1MB"))]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub timeout_ms: i64,
    pub max_wait_ms: i64,
    pub max_steps: u64,
    pub max_parallel: usize,
    pub result_in_memory: u64,
}

impl Limits {
    /// The flow's `limits`, with defaults, held within the caps
    pub fn of(flow: &Value) -> Limits {
        let limits = flow.get("limits");
        let field = |name: &str| limits.and_then(|l| l.get(name));
        let duration = |name: &str, default: i64, most: i64| field(name).and_then(Value::as_str).and_then(|t| parse_duration(t).ok()).unwrap_or(default).clamp(0, most);
        Limits {
            timeout_ms: duration("timeout", HOUR_MS, 7 * DAY_MS),
            max_wait_ms: duration("max_wait", 7 * DAY_MS, 90 * DAY_MS),
            max_steps: field("max_steps").and_then(Value::as_u64).unwrap_or(10_000).clamp(1, 1_000_000),
            max_parallel: field("max_parallel").and_then(Value::as_u64).unwrap_or(4).clamp(1, 64) as usize,
            result_in_memory: field("result_in_memory").and_then(Value::as_str).and_then(|t| parse_size(t).ok()).unwrap_or(1 << 20).clamp(1 << 10, 16 << 20),
        }
    }
}

/// A step's own `timeout`, else its kind's default; `None` for steps that hold other steps
pub fn step_timeout_ms(kind: &str, own: Option<&Value>) -> Option<i64> {
    if let Some(ms) = own.and_then(Value::as_str).and_then(|t| parse_duration(t).ok()) {
        return Some(ms);
    }
    match kind {
        "tool" | "call" | "ask" => Some(10 * 60_000),
        "agent" => Some(30 * 60_000),
        "code" => Some(60_000),
        _ => None,
    }
}
