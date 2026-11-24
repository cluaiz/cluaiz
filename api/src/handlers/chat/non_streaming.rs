use super::preflight::PreparedChatContext;
use super::types::ExternalChatRequest;
use crate::state::AppState;
use axum::response::{IntoResponse, Json};
use chrono::Utc;
use cluaiz_runtime::HostResponse;
use serde_json::json;
use std::sync::Arc;

pub async fn execute_non_streaming(
    state: Arc<AppState>,
    request: ExternalChatRequest,
    ctx: PreparedChatContext,
    dispatch_result: HostResponse,
) -> axum::response::Response {
    // keep_alive is about a loaded model: a provider's chat loaded none
    let keep_alive_val = request.keep_alive.filter(|_| ctx.provider.is_none());
    let req_session_id = request.session_id.clone();
    let turn_memory = super::preflight::TurnMemory::new(state.clone(), &ctx);

    let max_steps = super::api_turn::max_steps_for(&request);
    let mut turn = super::api_turn::ApiTurn::new(state.clone(), &ctx, &request, dispatch_result, None);
    let ended = match cluaiz_agents::turn::run(&mut turn, max_steps).await {
        Ok(ended) => ended,
        Err(_) => {
            if keep_alive_val == Some(0) {
                state.host.unload_model().await;
            }
            let (message, kind) = turn.error.clone().unwrap_or_else(|| ("The answer stopped with an error".into(), "engine_error"));
            return super::preflight::error_response(&request, &ctx.request_id, message, kind, None);
        }
    };
    // A client-run call ends the answer: OpenAI `tool_calls`, finish_reason "tool_calls"
    let client_call = turn.client_call.take();
    let calls_off = turn.calls_off;
    // Still `StepLimit` after the final no-tools answer (`ApiTurn::prepare_final_answer`): that answer made a call
    // too, which was not run, so only the text before it is the answer. Whole, its
    // `<tool_call>…</tool_call>` block would be taken as a reasoning block by `separate_reasoning`
    // (any leading `<tag>…</tag>`), so the call came back as `reasoning_content` with an empty
    // answer (run 52 `long_input`, run 57)
    let unrun_call = matches!(ended, cluaiz_agents::turn::Ended::StepLimit);
    let final_raw_text = if client_call.is_some() || unrun_call {
        turn.last_raw.split(cluaiz_core::tool_calls::OPEN).next().unwrap_or_default().to_string()
    } else {
        std::mem::take(&mut turn.last_raw)
    };
    let tool_events = std::mem::take(&mut turn.tool_events);
    let tool_turns = turn.tool_turns;

    // tool_choice "none": a `<tool_call>` block the model wrote anyway is answer text, as llama-server
    // returns it; whole, `separate_reasoning`'s generic fallback (any leading `<tag>…</tag>`) took it
    // as thinking and the answer came back empty (run 73)
    let (head, call_text) = match final_raw_text.find(cluaiz_core::tool_calls::OPEN) {
        Some(at) if calls_off => final_raw_text.split_at(at),
        _ => (final_raw_text.as_str(), ""),
    };
    // Extract reasoning and clean answer conforming to standard
    let (reasoning_opt, mut final_content, _) =
        cluaiz_models::StructuralDNA::separate_reasoning(
            head,
            ctx.dyn_think_start.as_deref(),
            ctx.dyn_think_end.as_deref(),
        );
    final_content.push_str(call_text);

    let mut message_json = json!({
        "role": "assistant",
        "content": final_content
    });
    if let Some(ref r_text) = reasoning_opt {
        message_json["reasoning_content"] = json!(r_text);
    }
    if let Some(calls) = &client_call {
        message_json["tool_calls"] = calls.clone();
    }

    // The engine's own count and stop reason for the last generation ("length" = max_tokens or context)
    let context_report = ctx.last_context(&state.host).await;
    let finish_reason = if client_call.is_some() {
        "tool_calls"
    } else {
        context_report.as_ref().map_or("stop", |report| report.finish_reason())
    };

    let mut response = json!({
        "id": ctx.request_id.clone(),
        "object": "chat.completion",
        "created": Utc::now().timestamp(),
        "model": ctx.resolved_model_name.clone(),
        "choices": [{
            "index": 0,
            "message": message_json,
            "finish_reason": finish_reason
        }]
    });

    // OpenAI sends `usage` with every non-streaming answer: the engine's own token counts
    let counts = super::turn_end::TurnCounts {
        completion_tokens: turn.token_count,
        reasoning_tokens: turn.reasoning_tokens,
        first_token_ms: turn.first_token_ms,
    };
    response["usage"] = super::turn_end::usage(&ctx, req_session_id.as_deref(), context_report.as_ref(), &counts);
    // Engine report (llama.cpp timings + load values) is always sent, like llama-server's "timings"
    response["engine"] = super::types::engine_report(&state.host).await;
    response["context_report"] = super::preflight::context_report(&ctx, &tool_events);

    super::turn_end::end_tools(&ctx, req_session_id.as_deref(), &tool_turns);
    // What the Engine added to this turn, so the next request renders it the same way
    if let Some(events) = super::preflight::record_turn(&state, &ctx, tool_turns.steps(), &final_content, &tool_events) {
        response["memory_events"] = events;
    }
    turn_memory.finish().await;

    super::turn_end::apply_keep_alive(&state, keep_alive_val).await;

    Json(response).into_response()
}
