pub mod types;
pub mod control;
pub mod preflight;
pub mod agent_loop;
pub mod non_streaming;

pub use types::*;
pub use control::*;

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;
use crate::state::AppState;

// ─── POST /v1/chat/completions (External Compatible API) ────────────
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExternalChatRequest>,
) -> axum::response::Response {
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    
    // 🛡️ API Boundary Input Validation: max_tokens must be >= 1 if provided
    if let Some(tokens) = request.max_tokens {
        if tokens == 0 {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": {
                        "message": "max_tokens must be greater than or equal to 1",
                        "type": "invalid_request_error",
                        "param": "max_tokens",
                        "code": "parameter_out_of_range"
                    }
                })),
            ).into_response();
        }
    }

    let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
    
    // Check if empty content should be prevented or trigger instant unload
    if last_message.flatten_to_string().await.is_empty() && request.keep_alive == Some(0) {
        tracing::info!("♻️ [Memory] Instant model unload requested via keep_alive: 0");
        let _ = state.dispatcher.unload_model().await;
        let empty_res = json!({
            "id": request_id.clone(),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": request.model.clone().unwrap_or_else(|| "default-model".to_string()),
            "choices": []
        });
        return Json(empty_res).into_response();
    }

    // 🚀 Execute Preflight Context Preparation
    let ctx = match preflight::PreparedChatContext::prepare(&state, &request, &request_id).await {
        Ok(c) => c,
        Err(err_res) => return err_res,
    };

    // Initial dispatch to LLM Kernel
    let dispatch_result = state.dispatcher.dispatch_stream(
        &ctx.json_prompt,
        ctx.skip_brain,
        ctx.active_model_path.clone(),
        ctx.validated_max_tokens,
    ).await;

    if request.stream {
        agent_loop::execute_streaming_loop(state, request, ctx, dispatch_result).await
    } else {
        non_streaming::execute_non_streaming(state, request, ctx, dispatch_result).await
    }
}
