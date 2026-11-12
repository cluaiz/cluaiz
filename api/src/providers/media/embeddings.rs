use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::{ignored_of, provider_error, send_to, with_headers};

/// `/v1/embeddings` for a provider's model: the OpenAI request (`input`, `encoding_format`,
/// `dimensions`, `user`) goes to `{base_url}/embeddings`, with `input_type` (`query` or `passage`,
/// which NVIDIA's and Voyage's query/document models need) and `truncate` when the caller sends
/// them; the answer comes back with `model` set to the full name. Engine memory and indexes keep
/// using the installed embedding model.
pub async fn forward_embeddings(target: Target, request: Value) -> axum::response::Response {
    if let Err(message) = target.check_kind("embedding") {
        return provider_error(&target, 400, message);
    }
    const FIELDS: &[&str] = &["model", "input", "encoding_format", "dimensions", "user", "input_type", "truncate"];
    let ignored = ignored_of(&request, FIELDS);
    let mut body = serde_json::Map::new();
    for field in FIELDS {
        if let Some(value) = request.get(*field).filter(|v| !v.is_null()) {
            body.insert((*field).into(), value.clone());
        }
    }
    body.insert("model".into(), json!(target.model));
    let response = match send_to(
        &target,
        "/embeddings",
        Value::Object(body).to_string().into(),
        "application/json",
    )
    .await
    {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let text = response.text().await.unwrap_or_default();
    let text = match serde_json::from_str::<Value>(&text) {
        Ok(mut value) if value.is_object() => {
            value["model"] = json!(target.full_name());
            value.to_string()
        }
        _ => text,
    };
    with_headers(
        axum::response::Response::builder()
            .status(200)
            .header(axum::http::header::CONTENT_TYPE, "application/json"),
        &target,
        &ignored,
    )
    .body(axum::body::Body::from(text))
    .unwrap_or_else(|_| {
        provider_error(
            &target,
            500,
            "the provider's answer could not be passed on".into(),
        )
    })
}
