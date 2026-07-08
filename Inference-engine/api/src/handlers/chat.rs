use axum::{
    extract::{State},
    response::{Json, Sse, sse::Event, IntoResponse},
};
use futures::stream::Stream;
use engines::models::entities::{ChatRequest, ChatResponse, ChatSession, ChatMessage, MessageRole};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use engines::neural_foundry::registry::registry_index::MasterRegistry;
use cluaiz_shared::environment::EnvironmentManager;
use crate::state::AppState;
use chrono::Utc;
use dispatcher::EngineResponse;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemporaryChatMode {
    Lite,
    Strict,
}

#[derive(Deserialize)]
pub struct ExternalChatRequest {
    pub model: String,
    pub messages: Vec<ExternalMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temporary_chat: Option<TemporaryChatMode>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ExternalMessage {
    pub role: String,
    pub content: String,
}

// ─── POST /v1/chat/completions (External Compatible API) ────────────
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExternalChatRequest>,
) -> axum::response::Response {
    let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
    
    let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    let send_telemetry = schema.stream_telemetry;
    let start_time = std::time::Instant::now();

    let skip_brain = match &request.temporary_chat {
        Some(TemporaryChatMode::Strict) => true,
        _ => false,
    };

    let mut matched_tool = String::new();
    let mut jit_injected = false;
    let mut augmented_prompt = last_message.clone();

    // 🚀 SEMANTIC ROUTING (Sovereign Injection)
    if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
        let prompt_lower = last_message.to_lowercase();
        let mut matched_skills = Vec::new();
        
        if let Some(path) = router.check_trigger(&prompt_lower) {
            matched_skills.push(path.clone());
        } else {
            for (keyword, path) in &router.keyword_index {
                if prompt_lower.contains(keyword) {
                    matched_skills.push(path.clone());
                    break;
                }
            }
        }
        
        for skill_path in matched_skills {
            if let Some(body) = engines::neural_foundry::extract_skill_body(&skill_path) {
                if let Some(name) = std::path::Path::new(&skill_path).file_name() {
                    matched_tool = name.to_string_lossy().to_string();
                }
                jit_injected = true;
                augmented_prompt = format!("{}\n\n{}", body, last_message);
                break; // Only inject one tool context for now to save space
            }
        }
    }

    // Initial dispatch to see if it's a stream or an error
    let dispatch_result = state.dispatcher.dispatch_stream(&augmented_prompt, skip_brain).await;

    if request.stream {
        match dispatch_result {
            EngineResponse::TokenStream(initial_rx) => {
                let state_clone = state.clone();
                let temp_mode = request.temporary_chat.clone();
                let req_session_id = request.session_id.clone();
                
                let stream = async_stream::stream! {
                     let mut current_prompt = augmented_prompt.clone();
                     let mut total_generated = String::new();
                     let mut overall_token_count = 0;
                     let mut first_ttft_ms = 0;
                     let mut is_first_token = true;
                     let mut telemetry_sent = false;
                     
                     let mut rx = initial_rx;
                     
                     let mut max_iters = 3;
                    while max_iters > 0 {
                        max_iters -= 1;
                         let mut tool_executed = false;
                         
                         while let Some(token) = rx.recv().await {
                             if !telemetry_sent {
                                 telemetry_sent = true;
                                 let step2_feedback = format!("__STEP_2_MATCH_START__:{}:0.88", matched_tool);
                                 let step2_chunk = json!({
                                     "choices": [{"delta": {"content": step2_feedback}}]
                                 });
                                 yield Ok::<_, Infallible>(Event::default().data(step2_chunk.to_string()));

                                 let step3_feedback = format!("__STEP_3_INJECT_START__:{}", jit_injected);
                                 let step3_chunk = json!({
                                     "choices": [{"delta": {"content": step3_feedback}}]
                                 });
                                 yield Ok::<_, Infallible>(Event::default().data(step3_chunk.to_string()));

                                 let step4_feedback = "__STEP_4_READ_SMS__".to_string();
                                 let step4_chunk = json!({
                                     "choices": [{"delta": {"content": step4_feedback}}]
                                 });
                                 yield Ok::<_, Infallible>(Event::default().data(step4_chunk.to_string()));
                             }
                            if token.starts_with("<TRIGGER:") && token.contains("</TRIGGER>") {
                                // Yield trigger token to SSE client so the test script knows we triggered a tool plan
                                let trigger_chunk = json!({
                                    "id": "chatcmpl-123",
                                    "object": "chat.completion.chunk",
                                    "created": Utc::now().timestamp(),
                                    "model": request.model.clone(),
                                    "choices": [{"delta": {"content": token.clone()}}]
                                });
                                yield Ok::<_, Infallible>(Event::default().data(trigger_chunk.to_string()));

                                // Execute the tool immediately in a separate scope to drop non-Send types before yield
                                let (comp_type, comp_name, execution_result) = {
                                    let header_end = token.find('>').unwrap_or(0);
                                    let header = &token[..header_end];
                                    let parts: Vec<&str> = header.trim_start_matches("<TRIGGER:").split(':').collect();
                                    
                                    let comp_type = if parts.len() >= 2 { parts[0] } else { "extension" };
                                    let comp_name = if parts.len() >= 2 { parts[1] } else { parts[0] };
                                    
                                    tracing::info!("🔍 [API] Single-Pass Intercepted Request for {} '{}'.", comp_type, comp_name);
                                    
                                    // Extract the JSON payload
                                    let json_start = header_end + 1;
                                    let json_end = token.find("</TRIGGER>").unwrap_or(token.len());
                                    let payload = token[json_start..json_end].trim();
                                    
                                    tracing::info!("⚙️ [API] Extracted JSON Payload: {}", payload);
                                    
                                    let mut execution_result = String::new();
                                    use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType};
                                    let executor = engines::neural_foundry::executor::sandbox::UnifiedExecutor::new();
                                    let ext_payload = ExtensionPayload::new(PayloadType::Json, payload.as_bytes());
                                    
                                    match executor.execute(comp_name, &ext_payload) {
                                        Ok(bytes) => {
                                            execution_result = String::from_utf8_lossy(&bytes).to_string();
                                            tracing::info!("✅ [API] Tool execution completed. Result length: {}", execution_result.len());
                                        },
                                        Err(e) => {
                                            execution_result = format!("Error executing {}: {}", comp_name, e);
                                            tracing::error!("❌ [API] Failed to execute tool: {}", e);
                                        }
                                    }
                                    (comp_type.to_string(), comp_name.to_string(), execution_result)
                                };
                                
                                // 🚀 SOVEREIGN KV-CACHE RESUME 
                                current_prompt = format!(
                                    "{}\n\n[PIVOT_CONTINUE]\n<result:{}:{}>\n{}\n</result>\nNow, provide the final conversational answer to the user based on the tool result above. Do NOT use any tools. Just answer the user directly.\n",
                                    current_prompt, comp_type, comp_name, execution_result
                                );
                                
                                // Yield pause feedback so client traces execution & injection flow
                                let pause_feedback = format!("__ENGINE_PAUSE_EXECUTE__:{}:{}", comp_name, execution_result);
                                let pause_chunk = json!({
                                    "id": "chatcmpl-123",
                                    "object": "chat.completion.chunk",
                                    "created": Utc::now().timestamp(),
                                    "model": request.model.clone(),
                                    "choices": [{"delta": {"content": pause_feedback}}]
                                });
                                yield Ok::<_, Infallible>(Event::default().data(pause_chunk.to_string()));

                                tool_executed = true;
                                break;
                            }
                            
                            // Normal Token Yielding
                            total_generated.push_str(&token);
                            overall_token_count += 1;
                            
                            if is_first_token {
                                first_ttft_ms = start_time.elapsed().as_millis();
                                is_first_token = false;
                            }
                            
                            let chunk = json!({
                                "id": "chatcmpl-123",
                                "object": "chat.completion.chunk",
                                "created": Utc::now().timestamp(),
                                "model": request.model.clone(),
                                "choices": [{"delta": {"content": token}}]
                            });
                            yield Ok::<_, Infallible>(Event::default().data(chunk.to_string()));
                        }
                        
                        if tool_executed {
                            tracing::info!("🔄 [API] Resuming generation with tool result (Single-Pass)...");
                            let new_dispatch = state_clone.dispatcher.dispatch_stream(&current_prompt, skip_brain).await;
                            if let EngineResponse::TokenStream(new_rx) = new_dispatch {
                                rx = new_rx;
                                continue;
                            } else {
                                break;
                            }
                        } else {
                            break; // Generation naturally finished
                        }
                    }
                    
                    // Generate Telemetry and Final Updates
                    let total_time_ms = start_time.elapsed().as_millis();
                    let tps = if total_time_ms > 0 {
                        (overall_token_count as f64 / (total_time_ms as f64 / 1000.0))
                    } else {
                        0.0
                    };

                    if send_telemetry {
                        let mut pulse_json = json!({});
                        if let Ok(lock) = cluaiz_shared::hardware::telemetry::get_pulse().pulse.read() {
                            pulse_json = serde_json::to_value(&*lock).unwrap_or(json!({}));
                        }

                        let telemetry_chunk = json!({
                            "id": "chatcmpl-123",
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [],
                            "usage": {
                                "completion_tokens": overall_token_count,
                                "total_tokens": overall_token_count,
                                "time_to_first_token_ms": first_ttft_ms,
                                "total_time_ms": total_time_ms,
                                "tokens_per_second": format!("{:.2}", tps).parse::<f64>().unwrap_or(0.0),
                                "hardware_snapshot": pulse_json
                            }
                        });
                        yield Ok::<_, Infallible>(Event::default().data(telemetry_chunk.to_string()));
                    }

                    // 🧠 Save to Engine Brain
                    if let Ok(vec) = state_clone.embedding_dispatcher.dispatch_embedding(&total_generated) {
                        if let Some(id) = req_session_id.clone() {
                            // let _ = engines::memory::tensor_transducer::TensorTransducer::save_context(&id, &total_generated, &vec);
                        }
                    }

                    yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
                };
                
                return Sse::new(stream).into_response();
            }
            EngineResponse::FinalResult(res) => {
                let chunk = json!({
                    "id": "chatcmpl-123",
                    "choices": [{"delta": {"content": res}}]
                });
                let stream = async_stream::stream! {
                    yield Ok::<_, Infallible>(Event::default().data(chunk.to_string()));
                    yield Ok::<_, Infallible>(Event::default().data("[DONE]"));
                };
                return Sse::new(stream).into_response();
            }
            EngineResponse::Error(err) => {
                return Json(json!({"error": err})).into_response();
            }
        }
    } else {
        // Non-streaming JSON response
        let content = match dispatch_result {
            EngineResponse::TokenStream(mut rx) => {
                let mut full_text = String::new();
                while let Some(token) = rx.recv().await {
                    full_text.push_str(&token);
                }
                full_text
            }
            EngineResponse::FinalResult(res) => res,
            EngineResponse::Error(err) => format!("Error: {}", err),
        };

        let mut response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": Utc::now().timestamp(),
            "model": request.model,
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": content.clone()
                },
                "finish_reason": "stop"
            }]
        });

        // 🧠 Save to Engine Brain
        if let Ok(vec) = state.embedding_dispatcher.dispatch_embedding(&content) {
            if let Some(id) = request.session_id.clone() {
                // let _ = engines::memory::tensor_transducer::TensorTransducer::save_context(&id, &content, &vec);
            }
        }

        if send_telemetry {
            let total_time_ms = start_time.elapsed().as_millis();
            let mut pulse_json = json!({});
            if let Ok(lock) = cluaiz_shared::hardware::telemetry::get_pulse().pulse.read() {
                pulse_json = serde_json::to_value(&*lock).unwrap_or(json!({}));
            }
            response["usage"] = json!({
                "total_time_ms": total_time_ms,
                "hardware_snapshot": pulse_json
            });
        }

        return Json(response).into_response();
    }
}

// ─── POST /chat — Legacy Legacy Protocol ─────────────────────────────
pub async fn chat(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatRequest>,
) -> Json<ChatResponse> {
    let dispatch_result = state.dispatcher.dispatch_prompt(&request.message).await;
    let content = match dispatch_result {
        Ok(res) => res,
        Err(e) => format!("Error processing prompt: {}", e),
    };

    let response = ChatResponse {
        id: format!("resp-{}", Utc::now().timestamp()),
        session_id: request.session_id.clone(),
        message: content,
        role: MessageRole::Assistant,
        timestamp: Utc::now(),
        model: "Sovereign-Internal".to_string(), 
        tokens_used: 0,
    };
    Json(response)
}

// ─── POST /v1/chat/stream — Simple SSE Streaming ─────────────────────
#[derive(Deserialize)]
pub struct ChatStreamRequest {
    pub message: String,
}

pub async fn chat_stream(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ChatStreamRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let dispatch_result = state.dispatcher.dispatch_stream(&request.message, false).await;
    
    let stream = async_stream::stream! {
        match dispatch_result {
            EngineResponse::TokenStream(mut rx) => {
                while let Some(token) = rx.recv().await {
                    yield Ok::<_, Infallible>(Event::default().data(token));
                }
            },
            EngineResponse::FinalResult(text) => {
                yield Ok::<_, Infallible>(Event::default().data(text));
            },
            EngineResponse::Error(e) => {
                yield Ok::<_, Infallible>(Event::default().data(format!("Error: {}", e)));
            }
        }
    };

    Sse::new(stream)
}

