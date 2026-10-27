//! 💬 Chat Completions API Handler (`api/src/handlers/chat/mod.rs`)
//! Full OpenAI-compatible chat controller with streaming, multi-turn agent tool execution, and thinking filters.

pub mod agent_loop;
pub(crate) mod authorize;
pub(crate) mod background;
pub(crate) mod context_plan;
pub(crate) mod ingest_events;
pub(crate) mod recall;
pub mod non_streaming;
pub mod preflight;
mod processing;
pub mod tool_parser;
mod tool_turns;
pub mod types;

use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

pub use tool_parser::{extract_tool_call, ExtractedToolCall};
pub use types::{
    ExternalChatRequest, ExternalMessage, MessageContent, TemporaryChatMode, DEFAULT_MAX_TOKENS,
};

/// `history: "engine"`: the chat's logged transcript goes in front of the new messages
pub(crate) fn engine_history(request: &mut types::ExternalChatRequest) {
    match request.storage.as_deref() {
        None | Some("engine") => {}
        Some("external") => match request.session_id.as_deref() {
            Some(chat) => cluaiz_agents::memory::register_external(chat),
            None => tracing::warn!("⚠️ [Memory] storage \"external\" needs a session_id; nothing is kept"),
        },
        Some("none") => request.temporary_chat = Some(types::TemporaryChatMode::Lite),
        Some(other) => tracing::warn!("⚠️ [Memory] Unknown storage '{}' ignored (engine, external or none)", other),
    }
    match request.history.as_deref() {
        None | Some("client") => {}
        Some("engine") => match request.session_id.as_deref().filter(|_| request.temporary_chat.is_none()) {
            Some(chat) => {
                let mut messages = ingest_events::history_messages(chat);
                messages.append(&mut request.messages);
                request.messages = messages;
            }
            None => tracing::warn!("⚠️ [Memory] history \"engine\" needs a session_id (and not a temporary chat); using the messages sent"),
        },
        Some(other) => tracing::warn!("⚠️ [Memory] Unknown history '{}' ignored (client or engine)", other),
    }
}

/// POST /v1/chat/completions (Full OpenAI-compatible endpoint)
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(mut request): Json<types::ExternalChatRequest>,
) -> Response {
    engine_history(&mut request);
    if request.messages.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": {
                    "message": "Messages array cannot be empty",
                    "type": "invalid_request_error"
                }
            })),
        )
            .into_response();
    }

    let req_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    if request.stream && request.return_progress {
        return processing::stream_with_progress(state, request, req_id).await;
    }

    let ctx =
        match preflight::PreparedChatContext::prepare(&state, &request, &req_id, None)
            .await
        {
            Ok(c) => c,
            Err(early_response) => return early_response,
        };

    // Model is loaded (or reused) inside preflight, before token budgets are computed
    let max_tokens = ctx.validated_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let initial_dispatch = ctx.dispatch(&state.host, None, max_tokens).await;

    if request.stream {
        agent_loop::execute_streaming_loop(state, request, ctx, initial_dispatch).await
    } else {
        non_streaming::execute_non_streaming(state, request, ctx, initial_dispatch).await
    }
}
