//! 💬 Chat Completions API Handler (`api/src/handlers/chat/mod.rs`)
//! Full OpenAI-compatible chat controller with streaming, multi-turn agent tool execution, and thinking filters.

pub mod agent_loop;
mod authorize;
pub(crate) mod context_plan;
pub(crate) mod ingest_events;
pub mod control;
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
use serde_json::{json, Value};
use std::sync::Arc;

pub use control::{cancel_chat_stream, skip_chat_reasoning};
pub use tool_parser::{extract_tool_call, ExtractedToolCall};
pub use types::{
    ExternalChatRequest, ExternalMessage, MessageContent, TemporaryChatMode, DEFAULT_MAX_TOKENS,
};

/// `history: "engine"`: the chat's logged transcript goes in front of the new messages
pub(crate) fn engine_history(request: &mut types::ExternalChatRequest) {
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
    headers: axum::http::HeaderMap,
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
        return processing::stream_with_progress(state, headers, request, req_id).await;
    }

    let ctx =
        match preflight::PreparedChatContext::prepare(&state, &request, &req_id, &headers, None)
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

#[derive(Debug, serde::Deserialize)]
pub struct ContextTelemetryQuery {
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub tools: Option<String>,
}

/// GET /v1/chat/context_telemetry
pub async fn get_context_telemetry(
    axum::extract::Query(query): axum::extract::Query<ContextTelemetryQuery>,
) -> Json<Value> {
    let model = query.model.unwrap_or_else(|| "default".to_string());
    let session_id = query.session_id.unwrap_or_else(|| "default".to_string());
    let active_tools = cluaiz_agents::ToolsEngine::get_active_tool_ids_for_session(&session_id);
    let telemetry = cluaiz_agents::ToolsEngine::compute_telemetry(
        &model,
        &session_id,
        &active_tools,
        0,
        0,
        0,
        0,
    );

    let val = serde_json::to_value(&telemetry).unwrap_or_default();
    Json(json!({
        "status": "success",
        "model": model,
        "session_id": session_id,
        "telemetry": val.clone(),
        "context_telemetry": val
    }))
}
