//! CEP errors: the JSON-RPC 2.0 codes, and the server codes in -32000..-32099 with a stable
//! `data.type` that clients switch on

use serde::Serialize;
use serde_json::{json, Value};

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;
pub const UNAUTHENTICATED: i32 = -32001;
pub const FORBIDDEN: i32 = -32002;
pub const APPROVAL_REQUIRED: i32 = -32003;
pub const NOT_FOUND: i32 = -32004;
pub const CONFLICT: i32 = -32005;
pub const TOO_LARGE: i32 = -32006;
pub const NOT_IMPLEMENTED: i32 = -32007;
pub const CANCELLED: i32 = -32008;
pub const TIMEOUT: i32 = -32009;
pub const UNAVAILABLE: i32 = -32010;

/// The `error` member of a response
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CepError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl CepError {
    pub fn new(code: i32, message: impl Into<String>, data: Option<Value>) -> Self {
        Self { code, message: message.into(), data }
    }

    /// A server error: `data.type` names it, `extra` adds fields to `data`
    fn typed(code: i32, kind: &str, message: impl Into<String>, extra: Value) -> Self {
        let mut data = json!({ "type": kind });
        if let (Some(data), Value::Object(extra)) = (data.as_object_mut(), extra) {
            data.extend(extra);
        }
        Self::new(code, message, Some(data))
    }

    pub fn parse_error() -> Self {
        Self::new(PARSE_ERROR, "Parse error", None)
    }

    pub fn invalid_request(reason: impl Into<String>) -> Self {
        Self::typed(INVALID_REQUEST, "invalid_request", reason, Value::Null)
    }

    pub fn method_not_found(method: &str, did_you_mean: Vec<String>, group_methods: Vec<String>) -> Self {
        Self::new(
            METHOD_NOT_FOUND,
            format!("Method not found: {method}"),
            Some(json!({ "did_you_mean": did_you_mean, "group_methods": group_methods })),
        )
    }

    /// `errors` holds `{path, message}` per problem; `example` is the method's example params
    pub fn invalid_params(message: impl Into<String>, errors: Vec<Value>, example: Option<Value>) -> Self {
        Self::typed(INVALID_PARAMS, "invalid_params", message, json!({ "errors": errors, "example": example }))
    }

    /// Details go to the log under `trace_id`, never to the client
    pub fn internal(trace_id: &str) -> Self {
        Self::new(INTERNAL_ERROR, "Internal error", Some(json!({ "trace_id": trace_id })))
    }

    pub fn unauthenticated() -> Self {
        Self::typed(UNAUTHENTICATED, "unauthenticated", "A valid Bearer token is required", Value::Null)
    }

    /// `needs` is the missing scope, "roots" or "owner"
    pub fn forbidden(needs: &str) -> Self {
        Self::typed(FORBIDDEN, "forbidden", format!("Not allowed: needs {needs}"), json!({ "needs": needs }))
    }

    pub fn approval_required(approval_id: &str, action: &str, target: &str, expires_at: &str) -> Self {
        Self::typed(
            APPROVAL_REQUIRED,
            "approval_required",
            "Approval required",
            json!({ "approval_id": approval_id, "action": action, "target": target, "expires_at": expires_at }),
        )
    }

    /// `kind` is the stable type, for example "file_not_found" or "chat_not_found"
    pub fn not_found(kind: &str, message: impl Into<String>) -> Self {
        Self::typed(NOT_FOUND, kind, message, Value::Null)
    }

    pub fn conflict(kind: &str, message: impl Into<String>) -> Self {
        Self::typed(CONFLICT, kind, message, Value::Null)
    }

    pub fn too_large(limit: u64, got: u64) -> Self {
        Self::typed(TOO_LARGE, "too_large", format!("Over the limit of {limit}"), json!({ "limit": limit, "got": got }))
    }

    /// Declared but not built: never a fake success
    pub fn not_implemented(what: &str) -> Self {
        Self::typed(NOT_IMPLEMENTED, "not_implemented", format!("{what} is not implemented yet"), Value::Null)
    }

    pub fn cancelled() -> Self {
        Self::typed(CANCELLED, "cancelled", "Cancelled", Value::Null)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::typed(TIMEOUT, "timeout", message, Value::Null)
    }

    pub fn unavailable(kind: &str, message: impl Into<String>) -> Self {
        Self::typed(UNAVAILABLE, kind, message, Value::Null)
    }

    /// The stable `data.type`, when the error has one
    pub fn kind(&self) -> Option<&str> {
        self.data.as_ref()?.get("type")?.as_str()
    }
}

impl std::fmt::Display for CepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for CepError {}
