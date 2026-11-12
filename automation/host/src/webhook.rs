//! `POST /v1/hooks/<flow>[/<trigger>]` (docs/automation/webhooks.md): outside services start a
//! flow by proving its secret. Served without the API token; every answer is in the audit log.

use crate::service::{self, Service};
use crate::triggers::{self, Fired};
use axum::body::to_bytes;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};
use cluaiz_api::AppState;
use cluaiz_automation::trigger::state::{self, Count};
use cluaiz_automation::trigger::webhook::{self, BodyError};
use cluaiz_automation::trigger::Owner;
use cluaiz_cep::{Audit, AuditEntry};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/hooks/{flow}", any(hook))
        .route("/v1/hooks/{flow}/{trigger}", any(hook))
        .route("/v1/hooks/runs/{run_id}/{token}", axum::routing::post(crate::reach::hook_route))
}

fn answer(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn error(status: StatusCode, kind: &str, message: &str) -> Response {
    answer(status, json!({ "error": { "type": kind, "message": message } }))
}

fn audit(name: &str, trigger: &str, remote: &Option<String>, status: StatusCode, started: Instant) {
    let method = format!("hooks/{name}/{trigger}");
    let digest = cluaiz_cep::approval::params_digest(&json!({ "remote": remote }));
    let code = (!status.is_success()).then_some(status.as_u16() as i32);
    let entry = AuditEntry { principal: "webhook", method: &method, scope: Some(cluaiz_cep::Scope::Exec), digest, code, ms: started.elapsed().as_millis() as u64 };
    cluaiz_security::audit::AuditFile::in_home().record(&entry);
}

async fn hook(State(state): State<Arc<AppState>>, Path(path): Path<HashMap<String, String>>, request: Request) -> Response {
    let started = Instant::now();
    let service = service::get(&state);
    let name = path.get("flow").cloned().unwrap_or_default();
    let remote = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string());
    let (status, response, trigger) = handle(&service, &name, path.get("trigger").map(String::as_str), request, &remote).await;
    if let Some(trigger) = &trigger {
        audit(&name, trigger, &remote, status, started);
    }
    tracing::info!("Automation: webhook {name} from {} answered {status}", remote.as_deref().unwrap_or("?"));
    response
}

async fn handle(service: &Arc<Service>, name: &str, trigger: Option<&str>, request: Request, remote: &Option<String>) -> (StatusCode, Response, Option<String>) {
    let refuse = |status: StatusCode, kind: &str, message: &str| (status, error(status, kind, message));
    if !service.triggers.ready.load(std::sync::atomic::Ordering::Relaxed) {
        let (s, r) = refuse(StatusCode::SERVICE_UNAVAILABLE, "automation_starting", "Automation is still loading flows");
        return (s, r, None);
    }
    // A disabled flow answers as one that does not exist, so the address tells nothing
    let Some((scope, id, hook)) = service.triggers.hook(name, trigger) else {
        let (s, r) = refuse(StatusCode::NOT_FOUND, "no_hook", "no enabled flow has a webhook at this address");
        return (s, r, None);
    };
    let flow = json!({ "name": name });
    let owner = Owner { paths: &service.paths, scope: scope.as_str(), flow: &flow };
    let refused = |status: StatusCode, kind: &str, message: &str| {
        state::count(&owner, &id, Count::Refused);
        let (s, r) = refuse(status, kind, message);
        (s, r, Some(id.clone()))
    };
    let method = request.method().clone();
    if !hook.methods.iter().any(|m| m == method.as_str()) {
        return refused(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed", &format!("this webhook takes {}", hook.methods.join(", ")));
    }
    if !service.triggers.take_hook_turn(name) {
        let (s, mut r, t) = refused(StatusCode::TOO_MANY_REQUESTS, "skipped", "too many requests for this flow this minute");
        r.headers_mut().insert("retry-after", "60".parse().expect("a number"));
        return (s, r, t);
    }
    let headers: HeaderMap = request.headers().clone();
    let too_long = headers.get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).is_some_and(|n| n > hook.max_body);
    if too_long {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "the body is larger than the webhook's max_body");
    }
    let query = request.uri().query().map(String::from);
    let Ok(raw) = to_bytes(request.into_body(), hook.max_body as usize).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "the body is larger than the webhook's max_body");
    };
    let secret = std::env::var(&hook.secret_env).unwrap_or_default();
    if secret.is_empty() {
        state::failed(&owner, &id, &format!("the secret env:{} is not set where the server runs, so every request is refused", hook.secret_env));
    }
    let header = |n: &str| headers.get(n).and_then(|v| v.to_str().ok()).map(String::from);
    if !hook.proves(&secret, &header, &raw) {
        return refused(StatusCode::UNAUTHORIZED, "bad_secret", "the secret or signature is missing or wrong");
    }
    let body = match webhook::body(header("content-type").as_deref(), &raw) {
        Ok(body) => body,
        Err(BodyError::Unsupported(message)) => return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported_media_type", &message),
        Err(BodyError::BadJson(message)) => return refused(StatusCode::BAD_REQUEST, "bad_body", &format!("the body is not JSON: {message}")),
    };
    let pairs: Vec<(String, String)> = headers.iter().filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string()))).collect();
    let data = webhook::data(&hook, method.as_str(), &pairs, query.as_deref(), body, remote.clone());
    let (status, response) = match triggers::fire(service, scope, name, &id, data).await {
        Fired::Started(launched) if hook.wait_for_result => result(service, &launched.task, &launched.run_id, launched.status, hook.respond_timeout_ms).await,
        Fired::Started(launched) => (StatusCode::ACCEPTED, answer(StatusCode::ACCEPTED, json!({ "accepted": true, "run_id": launched.run_id, "status": launched.status }))),
        Fired::Filtered => (StatusCode::OK, answer(StatusCode::OK, json!({ "accepted": false, "reason": "filtered" }))),
        Fired::Duplicate => (StatusCode::OK, answer(StatusCode::OK, json!({ "accepted": false, "reason": "duplicate" }))),
        Fired::Skipped(message) => {
            let mut r = error(StatusCode::TOO_MANY_REQUESTS, "skipped", &message);
            r.headers_mut().insert("retry-after", "60".parse().expect("a number"));
            (StatusCode::TOO_MANY_REQUESTS, r)
        }
        Fired::Gone => refuse(StatusCode::NOT_FOUND, "no_hook", "no enabled flow has a webhook at this address"),
        Fired::Failed(message) => refuse(StatusCode::INTERNAL_SERVER_ERROR, "flow_not_started", &message),
    };
    (status, response, Some(id))
}

/// `respond: result`: the run's outputs when it ends within the timeout, else `202`
async fn result(service: &Arc<Service>, task: &Value, run_id: &str, status: &str, timeout_ms: u64) -> (StatusCode, Response) {
    let accepted = || (StatusCode::ACCEPTED, answer(StatusCode::ACCEPTED, json!({ "accepted": true, "run_id": run_id, "status": status })));
    let Some(task_id) = task.get("task_id").and_then(Value::as_str) else { return accepted() };
    let deadline = Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u64;
        match service.state.cep.shared().tasks.result("owner", task_id, left).await {
            Ok(record) if record["status"] == "succeeded" => return (StatusCode::OK, answer(StatusCode::OK, record["outputs"].clone())),
            Ok(record) => {
                let error = record.get("error").cloned().unwrap_or(Value::Null);
                return (StatusCode::INTERNAL_SERVER_ERROR, answer(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": { "kind": error["kind"], "message": error["message"] }, "run_id": run_id })));
            }
            Err(e) if e.code == cluaiz_cep::error::TIMEOUT && Instant::now() < deadline => continue,
            Err(e) if e.code == cluaiz_cep::error::TIMEOUT => return accepted(),
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, answer(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": { "kind": "cancelled", "message": e.message }, "run_id": run_id }))),
        }
    }
}
