//! The JSON-RPC 2.0 envelope (https://www.jsonrpc.org/specification): requests, notifications,
//! batches and responses. One deviation, on purpose: params are by name only (an object), because
//! positional params are hard to read for people and small models (01-architecture §7.1).

use crate::error::CepError;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Protocol data inside `params._meta`, never method data
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    /// An approval id to use for this exact call
    #[serde(default)]
    pub approval: Option<String>,
    /// A client trace id, echoed in logs
    #[serde(default)]
    pub trace: Option<String>,
}

/// One valid request
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// `None` for a notification (no response is sent)
    pub id: Option<Value>,
    pub method: String,
    /// The params without `_meta`
    pub params: Map<String, Value>,
    pub meta: Meta,
}

/// One array item or the whole body: a request, or the error to answer with (`id` is null when it
/// could not be read)
pub type Item = Result<Request, (Value, CepError)>;

#[derive(Debug)]
pub enum Incoming {
    Single(Item),
    Batch(Vec<Item>),
}

/// Reads a body. `Err` is a reply for the whole body: a parse error, an empty batch, a batch over
/// `max_batch` items, or JSON nested deeper than `max_depth`.
pub fn parse(body: &[u8], max_batch: usize, max_depth: usize) -> Result<Incoming, CepError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| CepError::parse_error())?;
    if depth(&value) > max_depth {
        return Err(CepError::invalid_request(format!("JSON nested deeper than {max_depth}")));
    }
    match value {
        Value::Array(items) if items.is_empty() => Err(CepError::invalid_request("Empty batch")),
        Value::Array(items) if items.len() > max_batch => {
            Err(CepError::invalid_request(format!("Batch of {} is over the limit of {max_batch}", items.len())))
        }
        Value::Array(items) => Ok(Incoming::Batch(items.into_iter().map(request).collect())),
        other => Ok(Incoming::Single(request(other))),
    }
}

fn depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(depth).max().unwrap_or(0),
        Value::Object(map) => 1 + map.values().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

fn request(value: Value) -> Item {
    let Value::Object(mut object) = value else {
        return Err((Value::Null, CepError::invalid_request("A request must be an object")));
    };
    // The id is answered back whenever it is readable, even if the rest is invalid
    let id = match object.remove("id") {
        None => None,
        Some(id @ (Value::String(_) | Value::Number(_) | Value::Null)) => Some(id),
        Some(_) => return Err((Value::Null, CepError::invalid_request("`id` must be a string, a number or null"))),
    };
    let reply_id = id.clone().unwrap_or(Value::Null);
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return Err((reply_id, CepError::invalid_request("`jsonrpc` must be \"2.0\"")));
    }
    let method = match object.remove("method") {
        Some(Value::String(method)) => method,
        _ => return Err((reply_id, CepError::invalid_request("`method` must be a string"))),
    };
    let mut params = match object.remove("params") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(params)) => params,
        Some(_) => {
            let error = CepError::invalid_params("`params` must be an object (by name)", Vec::new(), None);
            return Err((reply_id, error));
        }
    };
    if let Some(key) = object.keys().find(|key| *key != "jsonrpc") {
        return Err((reply_id, CepError::invalid_request(format!("Unknown member `{key}`"))));
    }
    let meta = match params.remove("_meta") {
        None => Meta::default(),
        Some(meta) => serde_json::from_value(meta).map_err(|e| {
            (reply_id.clone(), CepError::invalid_params(format!("`_meta`: {e}"), Vec::new(), None))
        })?,
    };
    Ok(Request { id, method, params, meta })
}

pub fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn err(id: Value, error: &CepError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::*;

    fn single(body: &str) -> Item {
        match parse(body.as_bytes(), 32, 64).unwrap() {
            Incoming::Single(item) => item,
            Incoming::Batch(_) => panic!("batch"),
        }
    }

    #[test]
    fn test_spec_requests_and_notifications() {
        let call = single(r#"{"jsonrpc": "2.0", "method": "subtract", "params": {"subtrahend": 23, "minuend": 42}, "id": 3}"#).unwrap();
        assert_eq!((call.id, call.method.as_str(), call.params.len()), (Some(json!(3)), "subtract", 2));
        let note = single(r#"{"jsonrpc": "2.0", "method": "update", "params": {"a": 1}}"#).unwrap();
        assert_eq!(note.id, None);
        let null_id = single(r#"{"jsonrpc": "2.0", "method": "a", "id": null}"#).unwrap();
        assert_eq!(null_id.id, Some(Value::Null));
    }

    #[test]
    fn test_spec_errors() {
        assert_eq!(parse(br#"{"jsonrpc": "2.0", "method": "foobar, "params": "bar", "baz]"#, 32, 64).unwrap_err().code, PARSE_ERROR);
        let (id, error) = single(r#"{"jsonrpc": "2.0", "method": 1, "params": "bar"}"#).unwrap_err();
        assert_eq!((id, error.code), (Value::Null, INVALID_REQUEST));
        assert_eq!(parse(b"[]", 32, 64).unwrap_err().code, INVALID_REQUEST);
        let Incoming::Batch(items) = parse(b"[1,2,3]", 32, 64).unwrap() else { panic!() };
        assert!(items.iter().all(|i| matches!(i, Err((Value::Null, e)) if e.code == INVALID_REQUEST)));
        // Positional params: the one deviation from the spec
        let (id, error) = single(r#"{"jsonrpc": "2.0", "method": "a", "params": [1, 2], "id": 7}"#).unwrap_err();
        assert_eq!((id, error.code), (json!(7), INVALID_PARAMS));
        let (_, error) = single(r#"{"jsonrpc": "1.0", "method": "a", "id": 1}"#).unwrap_err();
        assert_eq!(error.code, INVALID_REQUEST);
    }

    #[test]
    fn test_meta_is_split_out_and_checked() {
        let call = single(r#"{"jsonrpc":"2.0","id":"x","method":"fs.write","params":{"path":"a","_meta":{"approval":"ap_1"}}}"#).unwrap();
        assert_eq!((call.params.contains_key("_meta"), call.meta.approval.as_deref()), (false, Some("ap_1")));
        let (_, error) = single(r#"{"jsonrpc":"2.0","id":1,"method":"a","params":{"_meta":{"aproval":"x"}}}"#).unwrap_err();
        assert_eq!(error.code, INVALID_PARAMS);
    }

    #[test]
    fn test_limits() {
        let batch = format!("[{}]", vec![r#"{"jsonrpc":"2.0","method":"a"}"#; 33].join(","));
        assert_eq!(parse(batch.as_bytes(), 32, 64).unwrap_err().code, INVALID_REQUEST);
        let deep = format!("{}{}", "[".repeat(70), "]".repeat(70));
        assert_eq!(parse(deep.as_bytes(), 32, 64).unwrap_err().code, INVALID_REQUEST);
    }
}
