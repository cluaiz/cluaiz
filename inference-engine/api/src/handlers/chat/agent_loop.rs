use axum::response::{sse::Event, IntoResponse, Sse};
use chrono::Utc;
use dispatcher::EngineResponse;
use serde_json::json;
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::state::AppState;
use super::preflight::PreparedChatContext;
use super::types::{ExternalChatRequest, StreamSignals, ACTIVE_STREAMS};

pub async fn execute_streaming_loop(
    state: Arc<AppState>,
    request: ExternalChatRequest,
    ctx: PreparedChatContext,
    initial_dispatch: EngineResponse,
) -> axum::response::Response {
    let initial_rx = match initial_dispatch {
        EngineResponse::TokenStream(rx) => rx,
        EngineResponse::FinalResult(res) => {
            let chunk = json!({
                "id": ctx.request_id.clone(),
                "choices": [{"delta": {"content": res}}]
            });
            let stream = async_stream::stream! {
                yield Ok::<_, Infallible>(Event::default().data(chunk.to_string()));
                yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
            };
            return Sse::new(stream).into_response();
        }
        EngineResponse::Error(err) => {
            let err_chunk = json!({
                "id": ctx.request_id.clone(),
                "choices": [{"delta": {"content": format!("Error: {}", err)}}]
            });
            let stream = async_stream::stream! {
                yield Ok::<_, Infallible>(Event::default().data(err_chunk.to_string()));
                yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
            };
            return Sse::new(stream).into_response();
        }
    };

    let state_clone = state.clone();
    let req_session_id = request.session_id.clone();
    let req_id_stream = ctx.request_id.clone();
    let keep_alive_val = request.keep_alive;
    
    // 🛡️ Multi-Tenant Stream Registry Registration
    let initial_skip_reasoning = request.skip_reasoning == Some(true) || ctx.active_think_mode == "off";
    if let Ok(mut lock) = ACTIVE_STREAMS.write() {
        lock.insert(
            req_id_stream.clone(),
            StreamSignals {
                cancel: Arc::new(AtomicBool::new(false)),
                skip_reasoning: Arc::new(AtomicBool::new(initial_skip_reasoning)),
            },
        );
    }

    let stream = async_stream::stream! {
        let mut current_prompt = ctx.json_prompt.clone();
        let mut total_generated = String::new();
        let mut overall_token_count = 0;
        let mut reasoning_tokens_count = 0usize;
        let mut think_filter = engine_core::metadata::dna::StreamingReasoningFilter::new(
            ctx.dyn_think_start.clone(),
            ctx.dyn_think_end.clone(),
            ctx.prompt_starts_in_think,
            ctx.active_think_mode == "off",
        );
        let mut first_ttft_ms = 0;
        let mut is_first_token = true;

        // 🚀 YIELD MODEL HEADER METADATA EARLY
        let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        if ctx.send_telemetry && permission.model_header_info {
            let loaded_models = super::types::generate_model_header_info();
            let early_header_chunk = json!({
                "id": req_id_stream.clone(),
                "object": "chat.completion.chunk",
                "created": Utc::now().timestamp(),
                "model": ctx.resolved_model_name.clone(),
                "choices": [],
                "usage": {
                    "model_header_info": {
                        "active_models": loaded_models
                    }
                }
            });
            yield Ok::<_, Infallible>(Event::default().data(early_header_chunk.to_string()));
        }

        // 📊 YIELD LIVE CONTEXT & HARDWARE TELEMETRY EARLY
        let mut active_tools_list = if let Some(ref sid) = req_session_id {
            crate::handlers::session_tools::get_active_tool_ids_for_session(sid)
        } else {
            Vec::new()
        };
        for tid in &ctx.active_tool_ids {
            if !active_tools_list.contains(tid) {
                active_tools_list.push(tid.clone());
            }
        }

        let context_snap = engines::tools::ToolsEngine::compute_telemetry(
            &ctx.resolved_model_name,
            req_session_id.as_deref().unwrap_or("default"),
            &active_tools_list,
            ctx.user_prompt_chars,
            ctx.history_chars,
            ctx.system_prompt_chars,
            0,
        );

        let early_context_chunk = json!({
            "id": req_id_stream.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": ctx.resolved_model_name.clone(),
            "choices": [],
            "usage": {
                "context_telemetry": context_snap,
                "active_session_tools": active_tools_list
            }
        });
        yield Ok::<_, Infallible>(Event::default().data(early_context_chunk.to_string()));
        
        let mut rx = initial_rx;
        
        // 🔄 AUTONOMOUS MULTI-TURN AGENT EXECUTION LOOP
        // Chaining consecutive tool executions up to 15 turns without halting prematurely
        let mut max_iters = 15;
        let mut agent_turn_history: Vec<(String, String)> = Vec::new();

        while max_iters > 0 {
            max_iters -= 1;
            let mut tool_executed = false;
            let mut token_accumulator = String::new();
            
            while let Some(token) = rx.recv().await {
                // 🛑 Real-time Multi-Stream Signal Verification (Cancel & Skip-Reasoning)
                let (should_cancel, should_skip) = if let Ok(lock) = ACTIVE_STREAMS.read() {
                    if let Some(signals) = lock.get(&req_id_stream) {
                        (signals.cancel.load(Ordering::Relaxed), signals.skip_reasoning.load(Ordering::Relaxed))
                    } else {
                        (false, false)
                    }
                } else {
                    (false, false)
                };

                if should_cancel {
                    tracing::info!("🛑 [StreamControl] Aborting stream '{}' on user cancel signal.", req_id_stream);
                    break;
                }
                if should_skip && think_filter.in_think_block {
                    tracing::debug!("⏩ [StreamControl] Skip reasoning signal active for stream '{}'.", req_id_stream);
                }

                if token.trim() == "[DONE]" {
                    break;
                }

                // 🧩 Token Accumulation & Robust Fragment Parsing for Tool Calls
                token_accumulator.push_str(&token);

                if token_accumulator.contains("<tool_call>") && token_accumulator.contains("</tool_call>") {
                    let call_start_idx = token_accumulator.rfind("<tool_call>").unwrap_or(0);
                    let clean_token = &token_accumulator[call_start_idx..];
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
                    tracing::info!("🔍 [API] Market Standard Tool Execution: {} '{}' with payload: {}", comp_type, public_call_name, payload_str);
                    
                    let tool_start_time = std::time::Instant::now();
                    let sec_mode = engines::tools::ToolsEngine::get_tool(&comp_name)
                        .ok()
                        .flatten()
                        .map(|t| t.security_mode)
                        .unwrap_or(engines::tools::registry::SecurityMode::Sandboxed);

                    let execution_result = match engines::tools::ToolsEngine::execute_tool_by_name(&comp_type, &comp_name, tool_sub_func.as_deref(), &payload_str).await {
                        Ok(res_str) => {
                            tracing::info!("✅ [API] Tool execution completed for '{}' ({} chars)", public_call_name, res_str.len());
                            res_str
                        },
                        Err(e) => {
                            tracing::error!("❌ [API] Failed to execute tool '{}': {}", public_call_name, e);
                            format!("Error executing {}: {}", public_call_name, e)
                        }
                    };
                    let latency_ms = tool_start_time.elapsed().as_secs_f64() * 1000.0;

                    // 1. Yield standard OpenAI tool_calls delta chunk
                    let tool_calls_chunk = json!({
                        "id": req_id_stream.clone(),
                        "object": "chat.completion.chunk",
                        "created": Utc::now().timestamp(),
                        "model": request.model.clone(),
                        "choices": [{
                            "index": 0,
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": format!("call_{}", comp_name),
                                    "type": "function",
                                    "function": {
                                        "name": public_call_name,
                                        "arguments": payload_str
                                    }
                                }]
                            }
                        }]
                    });
                    yield Ok::<_, Infallible>(Event::default().data(tool_calls_chunk.to_string()));

                    let mut execution_logs = vec![
                        format!("[ToolsEngine] Invoking {} '{}' (security: {:?})", comp_type, public_call_name, sec_mode),
                    ];
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&execution_result) {
                        if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
                            execution_logs.push(format!("[Process] Command: {}", cmd));
                        }
                        if let Some(exit_code) = val.get("exit_code") {
                            execution_logs.push(format!("[Process] Exit Code: {}", exit_code));
                        }
                        if let Some(stdout) = val.get("stdout").and_then(|v| v.as_str()) {
                            if !stdout.trim().is_empty() {
                                execution_logs.push(format!("[Process] Output captured ({} bytes)", stdout.len()));
                            }
                        }
                    }
                    execution_logs.push(format!("[ToolsEngine] Execution completed in {:.2}ms", latency_ms));

                    // 2. Yield rich cluaiz_tool_result status chunk
                    let result_chunk = json!({
                        "id": req_id_stream.clone(),
                        "object": "chat.completion.chunk",
                        "created": Utc::now().timestamp(),
                        "model": request.model.clone(),
                        "choices": [{
                            "index": 0,
                            "delta": {
                                "tool_result": {
                                    "id": format!("call_{}", comp_name),
                                    "name": public_call_name,
                                    "category": comp_type,
                                    "status": "completed",
                                    "security_mode": format!("{:?}", sec_mode).to_lowercase(),
                                    "latency_ms": ((latency_ms * 100.0).round() / 100.0),
                                    "input_payload": serde_json::from_str::<serde_json::Value>(&payload_str).unwrap_or(serde_json::json!(payload_str)),
                                    "output_result": serde_json::from_str::<serde_json::Value>(&execution_result).unwrap_or(serde_json::json!(&execution_result)),
                                    "logs": execution_logs,
                                    "result": execution_result.clone()
                                }
                            }
                        }]
                    });
                    yield Ok::<_, Infallible>(Event::default().data(result_chunk.to_string()));
                    
                    // 🔄 Continuous Multi-Turn Agent Turn Recording
                    let tool_call_xml = format!("<tool_call>\n{}\n</tool_call>", raw_json);
                    let tool_response_xml = engines::tools::ToolsEngine::format_tool_response_raw(
                        public_call_name,
                        &execution_result,
                    );
                    agent_turn_history.push((tool_call_xml, tool_response_xml));

                    // Construct cumulative agent history prompt
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
                    current_prompt = format!("[PIVOT_CONTINUE]{}", serde_json::to_string(&pivot_envelope).unwrap_or_default());

                    tool_executed = true;
                    token_accumulator.clear();
                    break;
                }
                
                // Normal Token Yielding & Reasoning Separation
                total_generated.push_str(&token);
                overall_token_count += 1;

                if is_first_token {
                    first_ttft_ms = ctx.start_time.elapsed().as_millis();
                    is_first_token = false;
                }

                let make_chunk = |content: Option<String>, reasoning: Option<String>| -> Event {
                    let mut delta = serde_json::Map::new();
                    if let Some(c) = content {
                        delta.insert("content".to_string(), serde_json::Value::String(c));
                    }
                    if let Some(r) = reasoning {
                        delta.insert("reasoning_content".to_string(), serde_json::Value::String(r));
                    }
                    let chunk = json!({
                        "id": req_id_stream.clone(),
                        "object": "chat.completion.chunk",
                        "created": Utc::now().timestamp(),
                        "model": ctx.resolved_model_name.clone(),
                        "choices": [{"delta": delta}]
                    });
                    Event::default().data(chunk.to_string())
                };

                let (content_opt, reasoning_opt) = think_filter.process_token(&token);
                if !should_skip && reasoning_opt.is_some() {
                    reasoning_tokens_count += 1;
                }
                if content_opt.is_some() || (!should_skip && reasoning_opt.is_some()) {
                    yield Ok::<_, Infallible>(make_chunk(content_opt, if should_skip { None } else { reasoning_opt }));
                }
            }
            
            if tool_executed {
                tracing::info!("🔄 [API] Resuming autonomous multi-turn generation with tool result...");
                let new_dispatch = state_clone.dispatcher.dispatch_stream(
                    &current_prompt,
                    ctx.skip_brain,
                    ctx.active_model_path.clone(),
                    ctx.validated_max_tokens,
                ).await;
                if let EngineResponse::TokenStream(new_rx) = new_dispatch {
                    rx = new_rx;
                    continue;
                } else {
                    break;
                }
            } else {
                break; // Generation naturally finished with no further tool calls
            }
        }

        // Flush any remaining characters in think_filter post-stream
        let should_skip = if let Ok(lock) = ACTIVE_STREAMS.read() {
            lock.get(&req_id_stream).map(|s| s.skip_reasoning.load(Ordering::Relaxed)).unwrap_or(false)
        } else {
            false
        };
        let (final_content, final_reasoning) = think_filter.flush_final();
        if !should_skip && final_reasoning.is_some() {
            reasoning_tokens_count += 1;
        }
        if final_content.is_some() || (!should_skip && final_reasoning.is_some()) {
            let mut delta = serde_json::Map::new();
            if let Some(c) = final_content {
                delta.insert("content".to_string(), serde_json::Value::String(c));
            }
            if let Some(r) = final_reasoning {
                if !should_skip {
                    delta.insert("reasoning_content".to_string(), serde_json::Value::String(r));
                }
            }
            yield Ok::<_, Infallible>(Event::default().data(json!({
                "id": req_id_stream.clone(),
                "object": "chat.completion.chunk",
                "created": Utc::now().timestamp(),
                "model": ctx.resolved_model_name.clone(),
                "choices": [{"delta": delta}]
            }).to_string()));
        }
        
        // Generate Telemetry and Final Updates
        let total_time_ms = ctx.start_time.elapsed().as_millis();
        let decode_time_ms = total_time_ms.saturating_sub(first_ttft_ms);
        let tps = if decode_time_ms > 30 {
            (overall_token_count as f64) / (decode_time_ms as f64 / 1000.0)
        } else if total_time_ms > 0 {
            (overall_token_count as f64) / (total_time_ms as f64 / 1000.0)
        } else {
            0.0
        };

        if ctx.send_telemetry {
            let prompt_tok_est = if ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars > 0 {
                ((ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars) / 4).max(1)
            } else {
                0
            };
            let mut usage_json = json!({
                "prompt_tokens": prompt_tok_est,
                "completion_tokens": overall_token_count,
                "total_tokens": overall_token_count + prompt_tok_est,
                "completion_tokens_details": {
                    "reasoning_tokens": reasoning_tokens_count
                },
                "time_to_first_token_ms": first_ttft_ms,
                "total_time_ms": total_time_ms,
                "tokens_per_second": format!("{:.2}", tps).parse::<f64>().unwrap_or(0.0)
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
                overall_token_count,
            );
            usage_json["context_telemetry"] = serde_json::to_value(ctx_telemetry).unwrap_or_default();

            if permission.model_header_info {
                let loaded_models = super::types::generate_model_header_info();
                usage_json["model_header_info"] = json!({
                    "active_models": loaded_models
                });
            }

            let telemetry_chunk = json!({
                "id": req_id_stream.clone(),
                "object": "chat.completion.chunk",
                "created": Utc::now().timestamp(),
                "model": ctx.resolved_model_name.clone(),
                "choices": [],
                "usage": usage_json
            });
            yield Ok::<_, Infallible>(Event::default().data(telemetry_chunk.to_string()));
        }

        // 🧠 Save to Engine Brain
        if let Ok(_vec) = state_clone.embedding_dispatcher.dispatch_embedding(&total_generated) {
            if let Some(_id) = req_session_id.clone() {
                // Future contextual vector indexing
            }
        }

        yield Ok::<_, Infallible>(Event::default().data("[DONE]"));

        // ⏳ Decrement session tool turns & purge expired ephemeral tools
        if let Some(ref sid) = req_session_id {
            crate::handlers::session_tools::decrement_session_turns(sid);
        }
        
        // 🧹 Auto-Cleanup: Remove completed stream from active registry
        if let Ok(mut lock) = ACTIVE_STREAMS.write() {
            lock.remove(&req_id_stream);
        }
        if keep_alive_val == Some(0) {
            tracing::info!("♻️ [Memory] Unloading model post-generation due to keep_alive: 0");
            let _ = state_clone.dispatcher.unload_model().await;
        } else if let Some(mins) = keep_alive_val {
            if mins > 0 {
                let secs = (mins as u64) * 60;
                let state_timer = Arc::clone(&state_clone);
                tracing::info!("⏳ [Memory] Scheduling model unload in {} minutes ({}s)", mins, secs);
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                    tracing::info!("♻️ [Memory] Unloading model post keep_alive timeout ({}m)", mins);
                    let _ = state_timer.dispatcher.unload_model().await;
                });
            }
        }
    };
    
    Sse::new(stream).into_response()
}
