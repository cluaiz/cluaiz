use super::preflight::PreparedChatContext;
use super::tool_parser;
use super::types::{ExternalChatRequest, DEFAULT_MAX_TOKENS};
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
    mut dispatch_result: HostResponse,
) -> axum::response::Response {
    let keep_alive_val = request.keep_alive;
    let req_session_id = request.session_id.clone();
    let start_time = ctx.start_time;

    let mut max_iters = 15;
    let mut agent_turn_history: Vec<(String, String)> = Vec::new();
    let mut final_raw_text = String::new();

    while max_iters > 0 {
        max_iters -= 1;

        let turn_content = match dispatch_result {
            HostResponse::TokenStream(mut rx) => {
                let mut full_text = String::new();
                while let Some(token) = rx.recv().await {
                    if let Some(error) = cluaiz_runtime::stream_error(&token) {
                        if keep_alive_val == Some(0) {
                            state.host.unload_model().await;
                        }
                        return super::preflight::error_response(
                            &request,
                            &ctx.request_id,
                            error.to_owned(),
                            super::types::engine_error_type(error),
                            None,
                        );
                    }
                    if token.trim() == "[DONE]" {
                        break;
                    }
                    full_text.push_str(&token);
                }
                full_text
            }
            HostResponse::Completed(res) => res,
            HostResponse::Error(err) => {
                return super::preflight::error_response(
                    &request,
                    &ctx.request_id,
                    err,
                    "engine_error",
                    None,
                )
            }
        };

        // Check if this turn called a tool
        if let Some(tool_call) = tool_parser::extract_tool_call(&turn_content) {
            let raw_json = &tool_call.raw_json;
            let raw_fn_name = tool_call.raw_fn_name.as_str();
            let payload_str = tool_call.payload_str;

            let resolved_target = if let Some(ref sid) = req_session_id {
                cluaiz_agents::ToolsEngine::resolve_function_target_for_session(sid, raw_fn_name)
                    .or_else(|| {
                        cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[])
                    })
            } else {
                cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[])
            };

            let (comp_type, comp_name, tool_sub_func) = if let Some(target) = resolved_target {
                (target.category, target.component_id, target.sub_function)
            } else {
                ("plugin".to_string(), raw_fn_name.to_string(), None)
            };

            let public_call_name = tool_sub_func.as_deref().unwrap_or(&comp_name);
            tracing::info!(
                "🔍 [API/NonStreaming] Tool Execution: {} '{}' with payload: {}",
                comp_type,
                public_call_name,
                payload_str
            );

            let execution_result = match cluaiz_agents::ToolsEngine::execute_tool_by_name(
                &comp_type,
                &comp_name,
                tool_sub_func.as_deref(),
                &payload_str,
            )
            .await
            {
                Ok(res_str) => {
                    tracing::info!(
                        "✅ [API/NonStreaming] Tool execution completed for '{}' ({} chars)",
                        public_call_name,
                        res_str.len()
                    );
                    res_str
                }
                Err(e) => {
                    tracing::error!(
                        "❌ [API/NonStreaming] Failed to execute tool '{}': {}",
                        public_call_name,
                        e
                    );
                    format!("Error executing {}: {}", public_call_name, e)
                }
            };

            let tool_call_xml = format!("<tool_call>\n{}\n</tool_call>", raw_json);
            let tool_response_xml = cluaiz_agents::ToolsEngine::format_tool_response_raw(
                public_call_name,
                &execution_result,
            );
            agent_turn_history.push((tool_call_xml, tool_response_xml));

            let mut cumulative_history = String::new();
            for (c_xml, r_xml) in &agent_turn_history {
                cumulative_history.push_str(c_xml);
                cumulative_history.push('\n');
                cumulative_history.push_str(r_xml);
                cumulative_history.push_str("\n\n");
            }

            let pivot_envelope = json!({
                "pivot_prompt": format!(
                    "<user_query>\n{}\n</user_query>\n{}\nReview the tool response(s) above. If sufficient information exists to fully answer the user query, synthesize the final response. Otherwise, continue and generate the next tool call as needed.\n",
                    ctx.initial_user_query, cumulative_history
                ),
                "samplers": ctx.samplers,
                "think_mode": &ctx.active_think_mode,
                "reasoning_effort": &ctx.active_reasoning_effort,
                "reasoning_budget_message": &ctx.reasoning_budget_message
            });
            let current_prompt = format!(
                "[PIVOT_CONTINUE]{}",
                serde_json::to_string(&pivot_envelope).unwrap_or_default()
            );

            tracing::info!("🔄 [API/NonStreaming] Resuming autonomous multi-turn generation with tool result...");
            dispatch_result = ctx
                .dispatch(
                    &state.host,
                    Some(&current_prompt),
                    ctx.validated_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
                )
                .await;
            continue;
        } else {
            final_raw_text = turn_content;
            break;
        }
    }

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

    // The engine's own count and stop reason for the last generation ("length" = max_tokens or context)
    let context_report = state.host.last_context().await;
    let finish_reason = context_report
        .as_ref()
        .map_or("stop", |report| report.finish_reason());

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

    // ⏳ Decrement session tool turns & purge expired ephemeral tools
    if let Some(ref sid) = req_session_id {
        crate::handlers::session_tools::decrement_session_turns(sid);
    }

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
