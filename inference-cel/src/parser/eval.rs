use super::ast::{CelValue};

/// Check whether a JSON payload's `field` falls within `[start, end]`.
/// Modeled directly after CDQL `eval_range` to natively filter before WASM.
pub fn eval_range(payload: &str, field: &str, start: &CelValue, end: &CelValue) -> bool {
    let json: serde_json::Value = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let field_val = match json.get(field) {
        Some(v) => v,
        None => return false,
    };

    match (start, end) {
        (CelValue::Number(lo), CelValue::Number(hi)) => {
            if let Some(num) = field_val.as_f64() {
                num >= *lo && num <= *hi
            } else {
                false
            }
        }
        (CelValue::Text(lo), CelValue::Text(hi)) => {
            if let Some(s) = field_val.as_str() {
                s >= lo.as_str() && s <= hi.as_str()
            } else {
                false
            }
        }
        _ => false,
    }
}
