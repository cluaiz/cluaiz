//! Embeddings, speech and transcription with a provider's model: the OpenAI request goes to the
//! provider, and the answer comes back in the shape of the Engine's own route.

mod embeddings;
mod speech;
mod transcription;

pub use embeddings::forward_embeddings;
pub use speech::forward_speech;
pub use transcription::forward_transcription;

use std::time::Duration;

use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::errors::{error_message, send_error};
use super::http::request;

/// A JSON error in the shape of the route's own errors
pub(super) fn provider_error(
    target: &Target,
    status: u16,
    message: String,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let status =
        axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
    (status, axum::Json(json!({ "error": { "message": message, "type": "provider_error", "provider": target.provider } }))).into_response()
}

/// One request to `{base_url}{path}`; Ok is the response with a 2xx status, Err the error reply
pub(super) async fn send_to(
    target: &Target,
    path: &str,
    body: reqwest::Body,
    content_type: &str,
) -> Result<reqwest::Response, axum::response::Response> {
    let url = format!("{}{path}", target.config.base_url.trim_end_matches('/'));
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url)
        .await
        .map_err(|message| provider_error(target, 400, message))?;
    let response = super::http::send(
        builder
            .timeout(Duration::from_secs(600))
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body),
        &target.config,
    )
    .await
    .map_err(|error| provider_error(target, 502, send_error(target, &error, key.as_deref())))?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(response);
    }
    let text = response.text().await.unwrap_or_default();
    Err(provider_error(
        target,
        status,
        error_message(target, status, &text, key.as_deref()),
    ))
}

pub(super) fn with_headers(
    mut reply: axum::http::response::Builder,
    target: &Target,
    ignored: &[String],
) -> axum::http::response::Builder {
    reply = reply.header("x-cluaiz-provider", target.provider.clone());
    if !ignored.is_empty() {
        reply = reply.header("x-cluaiz-ignored", ignored.join(","));
    }
    reply
}

/// Fields of `request` outside `known`, named in `X-Cluaiz-Ignored`
pub(super) fn ignored_of(request: &Value, known: &[&str]) -> Vec<String> {
    let mut ignored: Vec<String> = request
        .as_object()
        .map(|fields| {
            fields
                .iter()
                .filter(|(name, value)| !value.is_null() && !known.contains(&name.as_str()))
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default();
    ignored.sort();
    ignored
}
