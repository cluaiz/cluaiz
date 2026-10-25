//! The Engine side of CEP (Cluaiz Engine Protocol): the call context every method gets, the
//! method registry, and the `POST /v1/cep` endpoint. The protocol itself (envelope, registry,
//! approvals, tasks, discovery) is the `cluaiz-cep` crate. Design: `.agent/plans/CEP/`.

pub mod http;
pub mod mcp;
pub mod methods;
pub mod roots;

use crate::state::AppState;
use cluaiz_cep::{ApprovalStore, Engine, Limits, TaskStore};
use std::sync::Arc;
use std::time::Duration;

/// What a method gets besides its params: the Engine's state
#[derive(Clone)]
pub struct CallCtx {
    pub state: Arc<AppState>,
}

pub type CepEngine = Engine<CallCtx>;

/// How long the owner has to answer an approval request
const APPROVAL_TTL: Duration = Duration::from_secs(300);
/// Tasks: running at once per caller, kept after start by default, and at most
const MAX_RUNNING_TASKS: usize = 8;
const TASK_TTL_MS: u64 = 60 * 60 * 1000;
const MAX_TASK_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// A standard route's error response (the chat pipeline's) as a CEP error: the HTTP status picks
/// the code, the body's `error.message` and `error.type` are kept
pub async fn error_from_response(response: axum::response::Response) -> cluaiz_cep::CepError {
    use axum::http::StatusCode;
    use cluaiz_cep::CepError;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap_or_default();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    let error = &body["error"];
    let message = error["message"].as_str().or(body["message"].as_str()).unwrap_or("Request failed").to_string();
    let kind = error["type"].as_str().unwrap_or("request_failed");
    match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => CepError::invalid_params(message, Vec::new(), None),
        StatusCode::UNAUTHORIZED => CepError::unauthenticated(),
        StatusCode::FORBIDDEN => CepError::forbidden(kind),
        StatusCode::NOT_FOUND => CepError::not_found(kind, message),
        StatusCode::CONFLICT => CepError::conflict(kind, message),
        StatusCode::PAYLOAD_TOO_LARGE => CepError::new(cluaiz_cep::error::TOO_LARGE, message, Some(serde_json::json!({ "type": kind }))),
        _ => CepError::unavailable(kind, message),
    }
}

pub fn engine() -> CepEngine {
    Engine::new(
        methods::registry(),
        ApprovalStore::new(APPROVAL_TTL),
        TaskStore::new(MAX_RUNNING_TASKS, TASK_TTL_MS, MAX_TASK_TTL_MS),
        Limits::default(),
        "Cluaiz Engine Protocol",
        env!("CARGO_PKG_VERSION"),
    )
    .with_audit(Arc::new(cluaiz_security::audit::AuditFile::in_home()))
}
