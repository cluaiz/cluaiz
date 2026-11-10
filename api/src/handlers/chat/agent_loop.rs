use super::preflight::PreparedChatContext;
use super::types::{ExternalChatRequest, StreamSignals, ACTIVE_STREAMS};
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
                "error": {"message": err, "type": if ctx.provider.is_some() { "provider_error" } else { "engine_error" }}
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
    // keep_alive is about a loaded model: a provider's chat loaded none
    let keep_alive_val = request.keep_alive.filter(|_| ctx.provider.is_none());

    // 🛡️ Multi-Tenant Stream Registry Registration
    if let Ok(mut lock) = ACTIVE_STREAMS.write() {
        lock.insert(
            req_id_stream.clone(),
            StreamSignals {
                cancel: Arc::clone(&ctx.cancel_signal),
                skip_reasoning: Arc::clone(&ctx.skip_reasoning),
                // A provider's stream has no native reasoning control
                reasoning_supported: ctx.provider.is_none()
                    && ctx.active_think_mode != "off"
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
            cluaiz_agents::ToolsEngine::get_active_tool_ids_for_session(sid)
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
        // A tool the Engine loaded before the model answered (auto_load) shows like a load_tool call
        if let Some(step) = &ctx.auto_loaded {
            let payload = json!({ "id": ctx.tools_report["auto_loaded"] }).to_string();
            for delta in [
                json!({ "tool_calls": [{ "index": 0, "id": "call_tools", "type": "tools",
                    "function": { "name": "tools", "sub_function": cluaiz_agents::LOAD_TOOL_NAME, "arguments": payload, "auto": true } }] }),
                json!({ "tool_result": { "id": "call_tools", "name": "tools", "category": "tools", "status": "completed",
                    "input_payload": serde_json::from_str::<serde_json::Value>(&payload).unwrap_or_default(),
                    "result": step.result.clone(), "auto": true } }),
            ] {
                yield Ok::<_, Infallible>(Event::default().data(json!({
                    "id": req_id_stream.clone(),
                    "object": "chat.completion.chunk",
                    "created": Utc::now().timestamp(),
                    "model": request.model.clone(),
                    "choices": [{ "index": 0, "delta": delta }]
                }).to_string()));
            }
        }

        // The turn (cluaiz_agents::turn) runs here; its chunks arrive on `chunks` as it goes
        let (sink, mut chunks) = tokio::sync::mpsc::unbounded_channel::<String>();
        let max_steps = super::api_turn::max_steps_for(&request);
        let mut turn = super::api_turn::ApiTurn::new(state_clone.clone(), &ctx, &request, HostResponse::TokenStream(initial_rx), Some(sink));
        let outcome = {
            let core = cluaiz_agents::turn::run(&mut turn, max_steps);
            tokio::pin!(core);
            loop {
                let next = tokio::select! {
                    biased;
                    Some(data) = chunks.recv() => Ok(data),
                    outcome = &mut core => Err(outcome),
                };
                match next {
                    Ok(data) => yield Ok::<_, Infallible>(Event::default().data(data)),
                    Err(outcome) => break outcome,
                }
            }
        };
        while let Ok(data) = chunks.try_recv() {
            yield Ok::<_, Infallible>(Event::default().data(data));
        }
        if outcome.is_err() {
            let (message, kind) = turn.error.clone().unwrap_or_else(|| ("The answer stopped with an error".into(), "engine_error"));
            if let Some(p) = &ctx.progress { p.finish(cluaiz_core::processing::ProcessingState::Failed); }
            yield Ok::<_, Infallible>(Event::default().data(json!({
                "id": req_id_stream.clone(),
                "error": {"message": message, "type": kind}
            }).to_string()));
            if let Ok(mut lock) = ACTIVE_STREAMS.write() { lock.remove(&req_id_stream); }
            if keep_alive_val == Some(0) { ctx.unload_after(&state_clone.host).await; }
            yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
            return;
        }
        if matches!(outcome, Ok(cluaiz_agents::turn::Ended::StepLimit)) {
            tracing::info!("🔁 [API] Turn '{}' stopped at step/repeat limit (max_steps={})", req_id_stream, max_steps);
        }
        let client_call = turn.client_call.is_some();
        turn.flush();
        for data in std::iter::from_fn(|| chunks.try_recv().ok()) {
            yield Ok::<_, Infallible>(Event::default().data(data));
        }
        let overall_token_count = turn.token_count;
        let reasoning_tokens_count = turn.reasoning_tokens;
        let first_ttft_ms = turn.first_token_ms;
        let visible_answer = std::mem::take(&mut turn.visible_answer);
        let tool_events = std::mem::take(&mut turn.tool_events);
        let tool_turns = turn.tool_turns;

        // OpenAI's closing chunk: why the answer ended ("length" = max_tokens or the context)
        let context_report = ctx.last_context(&state_clone.host).await;
        let finish_reason = if client_call {
            "tool_calls"
        } else {
            context_report.as_ref().map_or("stop", |report| report.finish_reason())
        };
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

        // ⏳ Decrement session tool turns & purge expired ephemeral tools
        if let Some(ref sid) = req_session_id {
            cluaiz_agents::ToolsEngine::decrement_session_turns(sid);
        }
        super::tool_turns::end_turn_tools(&ctx, req_session_id.as_deref(), &tool_turns);
        // What the Engine added to this turn, so the next request renders it the same way (before
        // the closing chunk, which carries the new events for apps that keep the chat themselves)
        let memory_events = super::preflight::record_turn(&state_clone, &ctx, tool_turns.steps(), &visible_answer, &tool_events);

        // Engine report (llama.cpp timings + load values) is always sent, like llama-server's "timings"
        let mut engine_chunk = json!({
            "id": req_id_stream.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": ctx.resolved_model_name.clone(),
            "choices": [],
            "engine": super::types::engine_report(&state_clone.host).await,
            "context_report": super::preflight::context_report(&ctx, &tool_events)
        });
        if let Some(events) = memory_events {
            engine_chunk["memory_events"] = events;
        }
        yield Ok::<_, Infallible>(Event::default().data(engine_chunk.to_string()));

        if let Some(p) = &ctx.progress {
            p.finish(if ctx.cancel_signal.load(Ordering::Acquire) { cluaiz_core::processing::ProcessingState::Cancelled }
                else { cluaiz_core::processing::ProcessingState::Completed });
        }
        yield Ok::<_, Infallible>(Event::default().data("[DONE]"));

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
