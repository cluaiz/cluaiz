use axum::response::{IntoResponse, Json};
use chrono::Utc;
use dispatcher::EngineResponse;
use serde_json::json;
use std::sync::Arc;
use crate::state::AppState;
use super::preflight::PreparedChatContext;
use super::types::ExternalChatRequest;

pub async fn execute_non_streaming(
    state: Arc<AppState>,
    request: ExternalChatRequest,
    ctx: PreparedChatContext,
    mut dispatch_result: EngineResponse,
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
            EngineResponse::TokenStream(mut rx) => {
                let mut full_text = String::new();
                while let Some(token) = rx.recv().await {
                    if token.trim() == "[DONE]" {
                        break;
                    }
                    full_text.push_str(&token);
                }
                full_text
            }
            EngineResponse::FinalResult(res) => res,
            EngineResponse::Error(err) => format!("Error: {}", err),
        };

        // Check if this turn called a tool
        if turn_content.contains("<tool_call>") && turn_content.contains("</tool_call>") {
            let call_start_idx = turn_content.rfind("<tool_call>").unwrap_or(0);
            let clean_token = &turn_content[call_start_idx..];
            let json_start = clean_token.find("<tool_call>").map(|i| i + 11).unwrap_or(0);
            let json_end = clean_token.find("</tool_call>").unwrap_or(clean_token.len());
            let raw_json = clean_token[json_start..json_end].trim();

            let parsed_json: serde_json::Value = serde_json::from_str(raw_json).unwrap_or_else(|_| {
                json!({ "raw": raw_json })
            });

            let raw_fn_name = parsed_json.get("name")
                .or_else(|| parsed_json.get("function"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let args_val = parsed_json.get("arguments")
                .or_else(|| parsed_json.get("parameters"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);

            let payload_str = if args_val.is_string() {
                args_val.as_str().unwrap().to_string()
            } else if !args_val.is_null() {
                serde_json::to_string(&args_val).unwrap_or_default()
            } else {
                serde_json::to_string(&parsed_json).unwrap_or_default()
            };

            let resolved_target = if let Some(ref sid) = req_session_id {
                engines::tools::ToolsEngine::resolve_function_target_for_session(sid, raw_fn_name)
                    .or_else(|| engines::tools::ToolsEngine::resolve_function_target(raw_fn_name, &[]))
            } else {
                engines::tools::ToolsEngine::resolve_function_target(raw_fn_name, &[])
            };

            let (comp_type, comp_name, tool_sub_func) = if let Some(target) = resolved_target {
                (target.category, target.component_id, target.sub_function)
            } else {
                ("plugin".to_string(), raw_fn_name.to_string(), None)
            };

            let public_call_name = tool_sub_func.as_deref().unwrap_or(&comp_name);
            tracing::info!("🔍 [API/NonStreaming] Tool Execution: {} '{}' with payload: {}", comp_type, public_call_name, payload_str);
            
            let execution_result = match engines::tools::ToolsEngine::execute_tool_by_name(&comp_type, &comp_name, tool_sub_func.as_deref(), &payload_str).await {
                Ok(res_str) => {
                    tracing::info!("✅ [API/NonStreaming] Tool execution completed for '{}' ({} chars)", public_call_name, res_str.len());
                    res_str
                },
                Err(e) => {
                    tracing::error!("❌ [API/NonStreaming] Failed to execute tool '{}': {}", public_call_name, e);
                    format!("Error executing {}: {}", public_call_name, e)
                }
            };

            let tool_call_xml = format!("<tool_call>\n{}\n</tool_call>", raw_json);
            let tool_response_xml = engines::tools::ToolsEngine::format_tool_response_raw(
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
                "samplers": {
                    "temp": ctx.effective_temp,
                    "top_p": ctx.effective_top_p,
                    "top_k": ctx.effective_top_k,
                    "min_p": ctx.effective_min_p,
                    "presence_penalty": ctx.effective_presence,
                    "frequency_penalty": ctx.effective_frequency,
                    "repeat_penalty": ctx.effective_repeat,
                    "seed": ctx.effective_seed
                },
                "think_mode": &ctx.active_think_mode,
                "response_length": &ctx.active_response_length
            });
            let current_prompt = format!("[PIVOT_CONTINUE]{}", serde_json::to_string(&pivot_envelope).unwrap_or_default());

            tracing::info!("🔄 [API/NonStreaming] Resuming autonomous multi-turn generation with tool result...");
            dispatch_result = state.dispatcher.dispatch_stream(
                &current_prompt,
                ctx.skip_brain,
                ctx.active_model_path.clone(),
                ctx.validated_max_tokens,
            ).await;
            continue;
        } else {
            final_raw_text = turn_content;
            break;
        }
    }

    // Extract reasoning and clean answer conforming to OpenAI / DeepSeek standard
    let (reasoning_opt, final_content, reasoning_toks) = engine_core::metadata::dna::StructuralDNA::separate_reasoning(
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

    let mut response = json!({
        "id": ctx.request_id.clone(),
        "object": "chat.completion",
        "created": Utc::now().timestamp(),
        "model": ctx.resolved_model_name.clone(),
        "choices": [{
            "index": 0,
            "message": message_json,
            "finish_reason": "stop"
        }]
    });

    // 🧠 Save to Engine Brain
    if let Ok(_vec) = state.embedding_dispatcher.dispatch_embedding(&final_content) {
        if let Some(_id) = req_session_id.clone() {
            // Future contextual vector indexing
        }
    }

    if ctx.send_telemetry {
        let total_time_ms = start_time.elapsed().as_millis();
        let comp_tok_est = if !final_content.is_empty() {
            (final_content.len() / 4).max(final_content.split_whitespace().count()).max(1)
        } else {
            0
        };
        let prompt_tok_est = if ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars > 0 {
            ((ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars) / 4).max(1)
        } else {
            0
        };
        let total_completion_tokens = comp_tok_est + reasoning_toks;
        let mut usage_json = json!({
            "prompt_tokens": prompt_tok_est,
            "completion_tokens": total_completion_tokens,
            "total_tokens": total_completion_tokens + prompt_tok_est,
            "completion_tokens_details": {
                "reasoning_tokens": reasoning_toks
            },
            "total_time_ms": total_time_ms
        });
        
        // 🌐 Real-Time Context & Memory Telemetry Breakdown
        let mut active_ids = req_session_id.as_deref()
            .map(engines::tools::ToolsEngine::get_active_tool_ids_for_session)
            .unwrap_or_default();
        for tid in &ctx.active_tool_ids {
            if !active_ids.contains(tid) {
                active_ids.push(tid.clone());
            }
        }
        let ctx_telemetry = engines::tools::ToolsEngine::compute_telemetry(
            &ctx.resolved_model_name,
            req_session_id.as_deref().unwrap_or("ephemeral"),
            &active_ids,
            ctx.user_prompt_chars,
            ctx.history_chars,
            ctx.system_prompt_chars,
            total_completion_tokens,
        );
        usage_json["context_telemetry"] = serde_json::to_value(ctx_telemetry).unwrap_or_default();
        
        let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        if permission.model_header_info {
            let loaded_models = super::types::generate_model_header_info();
            usage_json["model_header_info"] = json!({
                "active_models": loaded_models
            });
        }
        
        response["usage"] = usage_json;
    }

    // ⏳ Decrement session tool turns & purge expired ephemeral tools
    if let Some(ref sid) = req_session_id {
        crate::handlers::session_tools::decrement_session_turns(sid);
    }

    // 🛑 POST-GENERATION UNLOAD / KEEP_ALIVE TIMING
    if keep_alive_val == Some(0) {
        tracing::info!("♻️ [Memory] Unloading model post-generation (non-streaming) due to keep_alive: 0");
        let _ = state.dispatcher.unload_model().await;
    } else if let Some(mins) = keep_alive_val {
        if mins > 0 {
            let secs = (mins as u64) * 60;
            let state_timer = Arc::clone(&state);
            tracing::info!("⏳ [Memory] Scheduling model unload in {} minutes ({}s)", mins, secs);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                tracing::info!("♻️ [Memory] Unloading model post keep_alive timeout ({}m)", mins);
                let _ = state_timer.dispatcher.unload_model().await;
            });
        }
    }

    Json(response).into_response()
}
