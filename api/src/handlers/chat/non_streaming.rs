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
    let keep_alive_val = request.keep_alive;
    let req_session_id = request.session_id.clone();
    let start_time = ctx.start_time;
    let turn_memory = super::preflight::TurnMemory::new(state.clone(), &ctx);

    let mut turn = super::api_turn::ApiTurn::new(state.clone(), &ctx, &request, dispatch_result, None);
    if cluaiz_agents::turn::run(&mut turn, super::api_turn::MAX_STEPS).await.is_err() {
        if keep_alive_val == Some(0) {
            state.host.unload_model().await;
        }
        let (message, kind) = turn.error.clone().unwrap_or_else(|| ("The answer stopped with an error".into(), "engine_error"));
        return super::preflight::error_response(&request, &ctx.request_id, message, kind, None);
    }
    // A client-run call ends the answer: OpenAI `tool_calls`, finish_reason "tool_calls"
    let client_call = turn.client_call.take();
    let final_raw_text = if client_call.is_some() {
        turn.last_raw.split(cluaiz_core::tool_calls::OPEN).next().unwrap_or_default().to_string()
    } else {
        std::mem::take(&mut turn.last_raw)
    };
    let tool_events = std::mem::take(&mut turn.tool_events);
    let tool_turns = turn.tool_turns;

    // Extract reasoning and clean answer conforming to standard
    let (reasoning_opt, final_content, reasoning_toks) =
        cluaiz_models::StructuralDNA::separate_reasoning(
            &final_raw_text,
            ctx.dyn_think_start.as_deref(),
            ctx.dyn_think_end.as_deref(),
        );

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
    let context_report = state.host.last_context().await;
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

    if ctx.send_telemetry {
        let total_time_ms = start_time.elapsed().as_millis();
        let comp_tok_est = if !final_content.is_empty() {
            (final_content.len() / 4)
                .max(final_content.split_whitespace().count())
                .max(1)
        } else {
            0
        };
        let prompt_tok_est = match &context_report {
            Some(report) => report.prompt_tokens,
            None if ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars > 0 => {
                ((ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars) / 4).max(1)
            }
            None => 0,
        };
        let total_completion_tokens = comp_tok_est + reasoning_toks;
        let mut usage_json = json!({
            "prompt_tokens": prompt_tok_est,
            "completion_tokens": total_completion_tokens,
            "total_tokens": total_completion_tokens + prompt_tok_est,
            "completion_tokens_details": {
                "reasoning_tokens": reasoning_toks
            },
            "total_time_ms": total_time_ms,
            "model_load_ms": ctx.model_load_ms
        });

        // 🌐 Real-Time Context & Memory Telemetry Breakdown
        let mut active_ids = req_session_id
            .as_deref()
            .map(cluaiz_agents::ToolsEngine::get_active_tool_ids_for_session)
            .unwrap_or_default();
        for tid in &ctx.active_tool_ids {
            if !active_ids.contains(tid) {
                active_ids.push(tid.clone());
            }
        }
        let mut ctx_telemetry = cluaiz_agents::ToolsEngine::compute_telemetry(
            &ctx.resolved_model_name,
            req_session_id.as_deref().unwrap_or("ephemeral"),
            &active_ids,
            ctx.user_prompt_chars,
            ctx.history_chars,
            ctx.system_prompt_chars,
            total_completion_tokens,
        );
        // Totals from the engine's own count of what the context holds now
        if let Some(report) = &context_report {
            ctx_telemetry.context_breakdown.apply_measured(report.n_ctx, report.n_tokens);
        }
        usage_json["context_telemetry"] = serde_json::to_value(ctx_telemetry).unwrap_or_default();

        let permission = cluaiz_security::PermissionSchema::load();
        if permission.model_header_info {
            let loaded_models = super::types::generate_model_header_info();
            usage_json["model_header_info"] = json!({
                "active_models": loaded_models
            });
        }

        response["usage"] = usage_json;
    }
    // Engine report (llama.cpp timings + load values) is always sent, like llama-server's "timings"
    response["engine"] = super::types::engine_report(&state.host).await;
    response["context_report"] = super::preflight::context_report(&ctx, &tool_events);

    // ⏳ Decrement session tool turns & purge expired ephemeral tools
    if let Some(ref sid) = req_session_id {
        cluaiz_agents::ToolsEngine::decrement_session_turns(sid);
    }
    super::tool_turns::end_turn_tools(&ctx, req_session_id.as_deref(), &tool_turns);
    // What the Engine added to this turn, so the next request renders it the same way
    if let Some(events) = super::preflight::record_turn(&state, &ctx, tool_turns.steps(), &final_content, &tool_events) {
        response["memory_events"] = events;
    }
    turn_memory.finish().await;

    // 🛑 POST-GENERATION UNLOAD / KEEP_ALIVE TIMING
    if keep_alive_val == Some(0) {
        tracing::info!(
            "♻️ [Memory] Unloading model post-generation (non-streaming) due to keep_alive: 0"
        );
        state.host.unload_model().await;
    } else if let Some(mins) = keep_alive_val {
        if mins > 0 {
            let secs = (mins as u64) * 60;
            let state_timer = Arc::clone(&state);
            tracing::info!(
                "⏳ [Memory] Scheduling model unload in {} minutes ({}s)",
                mins,
                secs
            );
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                tracing::info!(
                    "♻️ [Memory] Unloading model post keep_alive timeout ({}m)",
                    mins
                );
                state_timer.host.unload_model().await;
            });
        }
    }

    Json(response).into_response()
}
