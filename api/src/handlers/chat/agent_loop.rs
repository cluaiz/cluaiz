use super::preflight::PreparedChatContext;
use super::tool_parser;
use super::types::{ExternalChatRequest, StreamSignals, ACTIVE_STREAMS, DEFAULT_MAX_TOKENS};
use crate::state::AppState;
use axum::response::{sse::Event, IntoResponse, Sse};
use chrono::Utc;
use cluaiz_runtime::HostResponse;
use serde_json::json;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub async fn execute_streaming_loop(
    state: Arc<AppState>,
    request: ExternalChatRequest,
    ctx: PreparedChatContext,
    initial_dispatch: HostResponse,
) -> axum::response::Response {
    let initial_rx = match initial_dispatch {
        HostResponse::TokenStream(rx) => rx,
        HostResponse::Completed(res) => {
            if let Some(p) = &ctx.progress {
                p.finish(cluaiz_core::processing::ProcessingState::Completed);
            }
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
        HostResponse::Error(err) => {
            if let Some(p) = &ctx.progress {
                p.finish(cluaiz_core::processing::ProcessingState::Failed);
            }
            let err_chunk = json!({
                "id": ctx.request_id.clone(),
                "error": {"message": err, "type": "engine_error"}
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
    if let Ok(mut lock) = ACTIVE_STREAMS.write() {
        lock.insert(
            req_id_stream.clone(),
            StreamSignals {
                cancel: Arc::clone(&ctx.cancel_signal),
                skip_reasoning: Arc::clone(&ctx.skip_reasoning),
                reasoning_supported: ctx.active_think_mode != "off"
                    && ctx
                        .dyn_think_start
                        .as_ref()
                        .is_some_and(|tag| !tag.is_empty())
                    && ctx
                        .dyn_think_end
                        .as_ref()
                        .is_some_and(|tag| !tag.is_empty()),
            },
        );
    }

    let turn_memory = super::preflight::TurnMemory::new(state.clone(), &ctx);
    let stream = async_stream::stream! {
        let turn_memory = turn_memory;
        let mut current_prompt = ctx.json_prompt.clone();
        let mut total_generated = String::new();
        let mut overall_token_count = 0;
        let mut reasoning_tokens_count = 0usize;
        let mut think_filter = cluaiz_models::StreamingReasoningFilter::new(
            ctx.dyn_think_start.clone(),
            ctx.dyn_think_end.clone(),
            // The engine sends the start tag itself when the rendered prompt opened the block
            false,
            ctx.active_think_mode == "off",
        );
        let mut call_text_guard = tool_parser::ToolCallTextGuard::default();
        let mut first_ttft_ms = 0;
        let mut is_first_token = true;

        // 🚀 YIELD MODEL HEADER METADATA EARLY
        let permission = cluaiz_security::PermissionSchema::load();
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

        let context_snap = cluaiz_agents::ToolsEngine::compute_telemetry(
            &ctx.resolved_model_name,
            req_session_id.as_deref().unwrap_or("default"),
            &active_tools_list,
            ctx.user_prompt_chars,
            ctx.history_chars,
            ctx.system_prompt_chars,
            0,
        );

        let mut early_context_chunk = json!({
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
        if ctx.prepared_media.is_some() {
            early_context_chunk["usage"]["context_telemetry_scope"] = json!("text_estimate_excluding_media; native prefill progress reports actual processed input tokens");
        }
        yield Ok::<_, Infallible>(Event::default().data(early_context_chunk.to_string()));

        // What the Engine chose about tools, before the first token
        yield Ok::<_, Infallible>(Event::default().data(json!({
            "id": req_id_stream.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": ctx.resolved_model_name.clone(),
            "choices": [],
            "context": { "tools": ctx.tools_report.clone() }
        }).to_string()));
        let mut tool_events: Vec<serde_json::Value> = Vec::new();

        let mut rx = initial_rx;

        // 🔄 AUTONOMOUS MULTI-TURN AGENT EXECUTION LOOP
        // Chaining consecutive tool executions up to 15 turns without halting prematurely
        let mut max_iters = 15;
        let mut tool_turns = super::tool_turns::ToolTurns::new(ctx.chat_tmpl.as_deref());

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
                if token.trim() == "[DONE]" {
                    break;
                }

                if let Some(error) = cluaiz_runtime::stream_error(&token) {
                    if let Some(p) = &ctx.progress { p.finish(cluaiz_core::processing::ProcessingState::Failed); }
                    yield Ok::<_, Infallible>(Event::default().data(json!({
                        "id": req_id_stream.clone(),
                        "error": {"message": error, "type": super::types::engine_error_type(error)}
                    }).to_string()));
                    if let Ok(mut lock) = ACTIVE_STREAMS.write() { lock.remove(&req_id_stream); }
                    if keep_alive_val == Some(0) { state_clone.host.unload_model().await; }
                    yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
                    return;
                }

                // 🧩 Token Accumulation & Robust Fragment Parsing for Tool Calls
                token_accumulator.push_str(&token);

                if let Some(tool_call) = tool_parser::extract_tool_call(&token_accumulator) {
                    // The call is complete: stop generating now (the worker stops at its next
                    // token), instead of after the tool ran or its approval was answered
                    rx.close();
                    let raw_json = &tool_call.raw_json;
                    let raw_fn_name = tool_call.raw_fn_name.as_str();
                    let payload_str = tool_call.payload_str;

                    // find_tools / load_tool / unload_tool are answered by the engine itself
                    // A tool called by its id before it was loaded is answered as load_tool
                    let implicit_load = super::tool_turns::needs_load(&ctx, &tool_turns, req_session_id.as_deref(), raw_fn_name);
                    let is_meta_tool = implicit_load
                        || (ctx.tool_load_room.is_some() && cluaiz_agents::ToolSearch::is_meta_tool(raw_fn_name));
                    let (meta_name, meta_payload) = if implicit_load {
                        (cluaiz_agents::LOAD_TOOL_NAME, json!({"id": raw_fn_name, "implicit": true}).to_string())
                    } else {
                        (raw_fn_name, payload_str.clone())
                    };
                    let resolved_target = if is_meta_tool {
                        None
                    } else if let Some(ref sid) = req_session_id {
                        cluaiz_agents::ToolsEngine::resolve_function_target_for_session(sid, raw_fn_name)
                            .or_else(|| cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[]))
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

                    let public_call_name = comp_name.clone();
                    tracing::info!("🔍 [API] Market Standard Tool Execution: {} '{}' (sub_func: {:?}) with payload: {}", comp_type, public_call_name, tool_sub_func, payload_str);

                    // Resolve tool SVG icon from filesystem/registry
                    let icon_svg = {
                        let env = cluaiz_core::EnvironmentManager::current();
                        let tool_dir = match comp_type.as_str() {
                            "skill" => env.skills_dir().join(&comp_name),
                            "plugin" => env.plugins_dir().join(&comp_name),
                            "mcp" => env.mcp_dir().join(&comp_name),
                            _ => env.tools_dir().join(&comp_name),
                        };
                        let p1 = tool_dir.join("assets").join("icon.svg");
                        let p2 = tool_dir.join("icon.svg");
                        if p1.exists() {
                            std::fs::read_to_string(p1).ok()
                        } else if p2.exists() {
                            std::fs::read_to_string(p2).ok()
                        } else {
                            None
                        }
                    };

                    // 1. Yield standard OpenAI tool_calls delta chunk EARLY before execution begins!
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
                                    "type": comp_type.clone(),
                                    "icon_svg": icon_svg.clone(),
                                    "function": {
                                        "name": public_call_name.clone(),
                                        "sub_function": tool_sub_func.clone(),
                                        "arguments": payload_str,
                                        "icon_svg": icon_svg.clone()
                                    }
                                }]
                            }
                        }]
                    });
                    yield Ok::<_, Infallible>(Event::default().data(tool_calls_chunk.to_string()));

                    let tool_start_time = std::time::Instant::now();
                    let sec_mode = cluaiz_agents::ToolsEngine::get_tool(&comp_name)
                        .ok()
                        .flatten()
                        .map(|t| t.security_mode)
                        .unwrap_or(cluaiz_agents::SecurityMode::Inherit);

                    // 🛡️ Human-In-The-Loop (HITL) Permission Gate (shared with the non-streaming path)
                    let assessment = super::authorize::assess(&comp_type, &comp_name, is_meta_tool);
                    let mut permission_granted = true;
                    let mut user_denial_feedback: Option<String> = None;

                    if assessment.requires_approval {
                        let pending = super::authorize::request(&state_clone, &comp_type, &public_call_name, &payload_str).await;

                        // Real-time permission request with the full parameters and capabilities
                        let perm_chunk = json!({
                            "id": req_id_stream.clone(),
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "permission_request": {
                                        "request_id": pending.request_id.clone(),
                                        "tool_name": public_call_name.clone(),
                                        "category": comp_type.clone(),
                                        "capabilities": assessment.capabilities,
                                        "parameters": serde_json::from_str::<serde_json::Value>(&payload_str).unwrap_or(serde_json::json!(payload_str)),
                                        "status": "pending_approval",
                                        "timeout_seconds": super::authorize::APPROVAL_TIMEOUT_SECS
                                    }
                                }
                            }]
                        });
                        yield Ok::<_, Infallible>(Event::default().data(perm_chunk.to_string()));

                        if let Err(reason) = super::authorize::wait(&state_clone, pending, &public_call_name).await {
                            permission_granted = false;
                            user_denial_feedback = Some(reason);
                        }
                    } else {
                        // Stage 1: Auto-approved / Safe inspection -> Tool is queued and authorized
                        let pending_chunk = json!({
                            "id": req_id_stream.clone(),
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_status": {
                                        "id": format!("call_{}", comp_name),
                                        "name": public_call_name.clone(),
                                        "category": comp_type.clone(),
                                        "status": "pending",
                                        "message": "Authorized. Queued for execution."
                                    }
                                }
                            }]
                        });
                        yield Ok::<_, Infallible>(Event::default().data(pending_chunk.to_string()));
                    }

                    let mut unloaded_tool: Option<String> = None;
                    let execution_result = if permission_granted {
                        // Stage 2: Tool execution actively running in background process
                        let running_chunk = json!({
                            "id": req_id_stream.clone(),
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_status": {
                                        "id": format!("call_{}", comp_name),
                                        "name": public_call_name.clone(),
                                        "category": comp_type.clone(),
                                        "status": "running",
                                        "message": "Executing tool in background process"
                                    }
                                }
                            }]
                        });
                        yield Ok::<_, Infallible>(Event::default().data(running_chunk.to_string()));

                        let executed = if is_meta_tool {
                            let outcome = super::tool_turns::run_meta_tool(&ctx, &tool_turns, req_session_id.as_deref(), meta_name, &meta_payload).await;
                            tool_events.push(super::preflight::tool_event(meta_name, &meta_payload, &outcome.text));
                            unloaded_tool = outcome.unloaded;
                            Ok(outcome.text)
                        } else {
                            cluaiz_agents::ToolsEngine::execute_tool_by_name(&comp_type, &comp_name, tool_sub_func.as_deref(), &payload_str).await
                        };
                        match executed {
                            Ok(res_str) => {
                                tracing::info!("✅ [API] Tool execution completed for '{}' ({} chars)", public_call_name, res_str.len());
                                res_str
                            },
                            Err(e) => {
                                tracing::error!("❌ [API] Failed to execute tool '{}': {}", public_call_name, e);
                                format!("Error executing {}: {}", public_call_name, e)
                            }
                        }
                    } else {
                        let reason = user_denial_feedback.unwrap_or_else(|| "User denied permission to execute this tool.".to_string());
                        super::authorize::denied_result(&reason)
                    };
                    let latency_ms = tool_start_time.elapsed().as_secs_f64() * 1000.0;

                    let status_str = if !permission_granted {
                        "denied"
                    } else if execution_result.starts_with("Error executing") {
                        "failed"
                    } else {
                        "completed"
                    };

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
                    execution_logs.push(format!("[ToolsEngine] Execution {} in {:.2}ms", status_str, latency_ms));

                    // Stage 3: Yield rich cluaiz_tool_result status chunk
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
                                    "icon_svg": icon_svg.clone(),
                                    "status": status_str,
                                    "security_mode": sec_mode.as_str(),
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

                    // The call and its result join the conversation once; the next step renders it again.
                    // An unloaded tool's earlier load result in this answer becomes a short note.
                    if let Some(id) = &unloaded_tool {
                        tool_turns.tombstone(id);
                    }
                    tool_turns.push(raw_json, raw_fn_name, &execution_result);
                    current_prompt = tool_turns.continuation(&ctx);

                    tool_executed = true;
                    token_accumulator.clear();
                    call_text_guard.clear();
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
                let content_opt = content_opt.and_then(|c| call_text_guard.push(&c));
                if !should_skip && reasoning_opt.is_some() {
                    reasoning_tokens_count += 1;
                }
                if content_opt.is_some() || (!should_skip && reasoning_opt.is_some()) {
                    yield Ok::<_, Infallible>(make_chunk(content_opt, if should_skip { None } else { reasoning_opt }));
                }
            }

            if tool_executed {
                tracing::info!("🔄 [API] Resuming autonomous multi-turn generation with tool result...");
                // 100% DRY: Reusable turn reset for seamless multi-turn reasoning stream
                think_filter.reset_turn(false);
                let new_dispatch = ctx.dispatch(
                    &state_clone.host, Some(&current_prompt),
                    ctx.validated_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
                ).await;
                if let HostResponse::TokenStream(new_rx) = new_dispatch {
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
        // Held text that never became a complete tool call is shown as answer text.
        let final_content = match (call_text_guard.flush(), final_content) {
            (Some(held), Some(rest)) => Some(held + &rest),
            (held, rest) => held.or(rest),
        };
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

        // OpenAI's closing chunk: why the answer ended ("length" = max_tokens or the context)
        let context_report = state_clone.host.last_context().await;
        let finish_reason = context_report.as_ref().map_or("stop", |report| report.finish_reason());
        yield Ok::<_, Infallible>(Event::default().data(json!({
            "id": req_id_stream.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": ctx.resolved_model_name.clone(),
            "choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]
        }).to_string()));

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
            let prompt_tok_est = match &context_report {
                Some(report) => report.prompt_tokens,
                None if ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars > 0 => {
                    ((ctx.user_prompt_chars + ctx.history_chars + ctx.system_prompt_chars) / 4).max(1)
                }
                None => 0,
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
                "model_load_ms": ctx.model_load_ms,
                "tokens_per_second": format!("{:.2}", tps).parse::<f64>().unwrap_or(0.0)
            });

            // 🌐 Real-Time Context & Memory Telemetry Breakdown
            let mut active_ids = req_session_id.as_deref()
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
                overall_token_count,
            );
            // Totals from the engine's own count of what the context holds now
            if let Some(report) = &context_report {
                ctx_telemetry.context_breakdown.apply_measured(report.n_ctx, report.n_tokens);
            }
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

        // Engine report (llama.cpp timings + load values) is always sent, like llama-server's "timings"
        let engine_chunk = json!({
            "id": req_id_stream.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": ctx.resolved_model_name.clone(),
            "choices": [],
            "engine": super::types::engine_report(&state_clone.host).await,
            "context_report": super::preflight::context_report(&ctx, &tool_events)
        });
        yield Ok::<_, Infallible>(Event::default().data(engine_chunk.to_string()));

        if let Some(p) = &ctx.progress {
            p.finish(if ctx.cancel_signal.load(Ordering::Acquire) { cluaiz_core::processing::ProcessingState::Cancelled }
                else { cluaiz_core::processing::ProcessingState::Completed });
        }
        yield Ok::<_, Infallible>(Event::default().data("[DONE]"));

        // ⏳ Decrement session tool turns & purge expired ephemeral tools
        if let Some(ref sid) = req_session_id {
            crate::handlers::session_tools::decrement_session_turns(sid);
        }
        // What the Engine added to this turn, so the next request renders it the same way
        super::preflight::record_turn(&ctx, tool_turns.steps());
        turn_memory.finish().await;

        // 🧹 Auto-Cleanup: Remove completed stream from active registry
        if let Ok(mut lock) = ACTIVE_STREAMS.write() {
            lock.remove(&req_id_stream);
        }
        if keep_alive_val == Some(0) {
            tracing::info!("♻️ [Memory] Unloading model post-generation due to keep_alive: 0");
            state_clone.host.unload_model().await;
        } else if let Some(mins) = keep_alive_val {
            if mins > 0 {
                let secs = (mins as u64) * 60;
                let state_timer = Arc::clone(&state_clone);
                tracing::info!("⏳ [Memory] Scheduling model unload in {} minutes ({}s)", mins, secs);
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                    tracing::info!("♻️ [Memory] Unloading model post keep_alive timeout ({}m)", mins);
                    state_timer.host.unload_model().await;
                });
            }
        }
    };

    Sse::new(stream).into_response()
}
