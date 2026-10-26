//! `POST /v1/cep`: a JSON-RPC 2.0 request or batch in, the reply out.
//! The route sits behind the auth middleware like every other route, so the owner's
//! `api_auth.required` setting decides who gets here (decision D9); whoever does is the owner.

use super::CallCtx;
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use cluaiz_cep::{envelope, CepError, Principal};
use serde_json::Value;
use std::sync::Arc;

fn is_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"))
}

fn json_reply(status: StatusCode, body: Vec<u8>) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

pub async fn handle(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    if !is_json(&headers) {
        let error = CepError::invalid_request("Content-Type must be application/json");
        let body = serde_json::to_vec(&envelope::err(Value::Null, &error)).unwrap_or_default();
        return json_reply(StatusCode::UNSUPPORTED_MEDIA_TYPE, body);
    }
    let ctx = CallCtx { state: state.clone() };
    match state.cep.call(ctx, Arc::new(Principal::owner()), &body).await {
        Some(reply) => json_reply(StatusCode::OK, reply),
        // Only notifications: nothing to answer
        None => StatusCode::ACCEPTED.into_response(),
    }
}
