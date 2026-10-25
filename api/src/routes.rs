//! 🗺️ Central Single-Page Master Routing Map (`api/src/routes.rs`)
//! All Cluaiz endpoints unified in one master routing table with zero omissions.

use crate::handlers;
use crate::AppState;
use axum::{
    extract::{DefaultBodyLimit, Request},
    middleware::{from_fn, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub fn build_api_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::ACCEPT,
            axum::http::HeaderName::from_static("x-cluaiz-session"),
            axum::http::HeaderName::from_static("x-caller"),
            axum::http::HeaderName::from_static("x-approval-id"),
        ])
        .allow_origin(tower_http::cors::Any);

    let chat = post(handlers::chat::chat_completions).layer(from_fn(attachment_body_limit));
    let ingest = post(handlers::ingest::file_ingest).layer(from_fn(attachment_body_limit));

    Router::new()
        // ── 1. Core Health & Info ──
        .route("/health", get(handlers::health::health_check))
        // CEP: every Engine-specific method (`.agent/plans/CEP/`)
        .route("/v1/cep", post(crate::cep::http::handle).layer(from_fn(attachment_body_limit)))
        // ── 2. External Streaming, Embeddings & Audio API ──
        .route("/v1/chat/completions", chat.clone())
        .route("/api/chat", chat)
        .route(
            "/v1/embeddings",
            post(handlers::embeddings::generate_embeddings).layer(from_fn(attachment_body_limit)),
        )
        .route(
            "/v1/audio/speech",
            post(handlers::audio::speech).layer(from_fn(attachment_body_limit)),
        )
        .route("/v1/audio/voices", get(handlers::audio::voices))
        .route("/v1/audio/languages", get(handlers::audio::languages))
        // Audio arrives inline as a data URI, so it gets the same size limit as chat attachments
        .route(
            "/v1/audio/transcriptions",
            post(handlers::audio::transcriptions).layer(from_fn(attachment_body_limit)),
        )
        .route(
            "/v1/audio/execute",
            post(handlers::audio::execute_audio).layer(from_fn(attachment_body_limit)),
        )
        // ── 3. External Compatible & Legacy Models API ──
        .route("/v1/models", get(handlers::models::v1_models))
        .route("/api/tags", get(handlers::models::tags))
        .route("/api/pull", post(handlers::models::pull_model))
        // MCP server over the CEP registry
        .route("/mcp", post(crate::cep::mcp::handle))
        // ── 12. Vector Ingest API ──
        .route("/v1/ingest", ingest.clone())
        .route("/v1/ingest/file", ingest)
        // ── 13. System Control, Telemetry & Processes ──
        .route(
            "/v1/system/auth/token/generate",
            post(handlers::permission::generate_auth_token),
        )
        .route(
            "/v1/system/auth/token/revoke",
            post(handlers::permission::revoke_auth_token),
        )
        // ── 15. Storage Control API ──
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::auth_middleware,
        ))
        .layer(cors)
        .with_state(state)
}

// Keep the ordinary JSON/text allowance in addition to the configured source bytes.
// Base64 transport is larger than the actual source; decoding/admission retain their own limits.
const JSON_ENVELOPE_BYTES: u64 = 2 * cluaiz_core::MIB;

/// JSON transport ceiling for bounded inline attachments, including base64 and envelope bytes.
pub fn inline_body_bytes(input_bytes: u64) -> usize {
    usize::try_from(
        input_bytes
            .div_ceil(3)
            .saturating_mul(4)
            .saturating_add(JSON_ENVELOPE_BYTES),
    )
    .unwrap_or(usize::MAX)
}

async fn attachment_body_limit(mut request: Request, next: Next) -> Response {
    let limit = inline_body_bytes(
        cluaiz_core::Settings::current()
            .media
            .limits
            .max_input_bytes(),
    );
    let too_large = || {
        crate::utils::task_error(
        axum::http::StatusCode::PAYLOAD_TOO_LARGE,
        "request_too_large",
        format!("Inline attachment JSON exceeds the {limit}-byte request limit; reduce attachments/history or use an authorized file/URL source"),
    )
    };
    if request
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > limit as u64)
    {
        return too_large();
    }
    // Apply per request so a settings update does not leave an old router ceiling behind.
    DefaultBodyLimit::max(limit).apply(&mut request);
    let response = next.run(request).await;
    if response.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        too_large()
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_body_bytes_accounts_for_base64_padding_and_overflow() {
        assert_eq!(inline_body_bytes(0), JSON_ENVELOPE_BYTES as usize);
        assert_eq!(inline_body_bytes(1), JSON_ENVELOPE_BYTES as usize + 4);
        assert_eq!(inline_body_bytes(3), JSON_ENVELOPE_BYTES as usize + 4);
        assert_eq!(inline_body_bytes(4), JSON_ENVELOPE_BYTES as usize + 8);
        assert_eq!(inline_body_bytes(u64::MAX), usize::MAX);
    }
}
