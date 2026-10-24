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
    let turn_memory = super::preflight::TurnMemory::new(state.clone(), &ctx);

    let mut max_iters = 15;
    let mut tool_turns = super::tool_turns::ToolTurns::new(ctx.tool_role);
    let mut tool_events: Vec<serde_json::Value> = Vec::new();
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
                    // A complete tool call ends this step: stop generating at the next token
                    if token.contains('>') && tool_parser::extract_tool_call(&full_text).is_some() {
                        rx.close();
                    }
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

            // find_tools / load_tool / unload_tool are answered by the engine itself
            // A tool called by its id before it was loaded is answered as load_tool
            let implicit_load =
                super::tool_turns::needs_load(&ctx, &tool_turns, req_session_id.as_deref(), raw_fn_name);
            let is_meta_tool = implicit_load
                || (ctx.tool_load_room.is_some() && cluaiz_agents::ToolSearch::is_meta_tool(raw_fn_name))
                || ((ctx.memory_tools || raw_fn_name == super::recall::TASK_UPDATE_TOOL) && ctx.ledger_turn.is_some() && super::recall::is_memory_tool(raw_fn_name));
            let (meta_name, meta_payload) = if implicit_load {
                (cluaiz_agents::LOAD_TOOL_NAME, json!({"id": raw_fn_name, "implicit": true}).to_string())
            } else {
                (raw_fn_name, payload_str.clone())
            };
            let resolved_target = if is_meta_tool {
                None
            } else if let Some(ref sid) = req_session_id {
                cluaiz_agents::ToolsEngine::resolve_function_target_for_session(sid, raw_fn_name)
                    .or_else(|| {
                        cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[])
                    })
            } else {
                cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[])
            };

            let (comp_type, comp_name, tool_sub_func) = if is_meta_tool {
                ("tools".to_string(), raw_fn_name.to_string(), None)
            } else if let Some(target) = resolved_target {
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

            // Same approval gate as streaming: the call waits for the user's decision, which the
            // App sees under /v1/system/permission/pending
            // Searching other chats asks the user first (policy cross_chat: ask), like a tool call
                    let assessment = if is_meta_tool && super::recall::needs_cross_chat_approval(meta_name, &meta_payload) {
                        super::authorize::Assessment { requires_approval: true, capabilities: vec!["memory:other_chats".to_string()] }
                    } else {
                        super::authorize::assess(&comp_type, &comp_name, is_meta_tool, ctx.chat_id.as_deref())
                    };
            let denial = if assessment.requires_approval {
                let pending = super::authorize::request(&state, &comp_type, public_call_name, &payload_str, ctx.chat_id.as_deref()).await;
                tracing::info!(
                    "🛡️ [API/NonStreaming] Waiting for approval of '{}' ({})",
                    public_call_name,
                    pending.request_id
                );
                let decision = super::authorize::wait(&state, pending, public_call_name).await;
                tool_events.push(super::authorize::approval_event(public_call_name, &decision));
                decision.err()
            } else {
                None
            };

            let mut unloaded_tool: Option<String> = None;
            let executed = if let Some(reason) = denial {
                Ok(super::authorize::denied_result(&reason))
            } else if is_meta_tool {
                let outcome = super::tool_turns::run_meta_tool(&ctx, &tool_turns, req_session_id.as_deref(), meta_name, &meta_payload).await;
                tool_events.push(super::preflight::tool_event(meta_name, &meta_payload, &outcome.text));
                unloaded_tool = outcome.unloaded;
                Ok(outcome.text)
            } else {
                cluaiz_agents::ToolsEngine::execute_tool_by_name(
                    &comp_type,
                    &comp_name,
                    tool_sub_func.as_deref(),
                    &payload_str,
                )
                .await
            };
            let execution_result = match executed {
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

            if let Some(id) = &unloaded_tool {
                tool_turns.tombstone(id);
            }
            tool_turns.push(raw_json, public_call_name, &execution_result);
            let current_prompt = tool_turns.continuation(&ctx);

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
    response["context_report"] = super::preflight::context_report(&ctx, &tool_events);

    // ⏳ Decrement session tool turns & purge expired ephemeral tools
    if let Some(ref sid) = req_session_id {
        crate::handlers::session_tools::decrement_session_turns(sid);
    }
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
