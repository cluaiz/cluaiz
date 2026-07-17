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
    #[serde(default)]
    pub keep_alive: Option<i32>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ExternalMessage {
    pub role: String,
    pub content: String,
}

// ─── HELPER: Fetch Real Model Header Information ────────────
fn generate_model_header_info() -> Vec<Value> {
    let registry = cluaiz_shared::hardware::governor::HardwareGovernor::load_process_registry();
    let mut loaded_models = Vec::new();
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let roots = vec![env.local_dir.join("models"), env.global_dir.join("models")];
    let categories = ["chat", "embedding", "vision", "audio", "code"];

    for (_, info) in registry {
        let mut think_start = String::new();
        let mut think_close = String::new();
        let mut all_metadata = json!({});
        let mut context_window_total = info.context_size; // default to allocated

        let mut probed = false;
        
        // Search for the model file
        for root in &roots {
            if probed { break; }
            for category in &categories {
                if probed { break; }
                let cat_dir = root.join(category);
                if let Ok(dirs) = std::fs::read_dir(&cat_dir) {
                    for d in dirs.flatten() {
                        if let Ok(files) = std::fs::read_dir(d.path()) {
                            for f in files.flatten() {
                                let p = f.path();
                                let fname = p.file_name().unwrap_or_default().to_string_lossy();
                                if fname == info.model_id && p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                                    if let Ok((meta, _tensors, _count)) = cluaiz_shared::utils::GGUFProber::probe(&p) {
                                        let mut meta_map = serde_json::Map::new();
                                        for (k, v) in meta {
                                            meta_map.insert(k.clone(), Value::String(v.clone()));
                                            // Heuristics for context length
                                            if k.contains("context_length") {
                                                if let Ok(ctx) = v.parse::<usize>() {
                                                    context_window_total = ctx;
                                                }
                                            }
                                            // Try to find thinking tags in metadata directly
                                            if k.contains("think_start") || k.contains("thought_start") {
                                                think_start = v.clone();
                                            }
                                            if k.contains("think_close") || k.contains("think_end") || k.contains("thought_end") {
                                                think_close = v.clone();
                                            }
                                        }
                                        all_metadata = Value::Object(meta_map);
                                        probed = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        loaded_models.push(json!({
            "model_id": info.model_id,
            "engine": info.engine,
            "context_window_total": context_window_total,
            "context_window_allocated": info.context_size,
            "think_start_tag": think_start,
            "think_close_tag": think_close,
            "raw_header": all_metadata
        }));
    }
    
    loaded_models
}

// ─── POST /v1/chat/completions (External Compatible API) ────────────
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExternalChatRequest>,
) -> axum::response::Response {
    let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
    
    // 🛑 INSTANT UNLOAD LOGIC
    if last_message.is_empty() && request.keep_alive == Some(0) {
        tracing::info!("♻️ [Memory] Instant model unload requested via keep_alive: 0");
        let _ = state.dispatcher.unload_model().await;
        let empty_res = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": request.model.clone(),
            "choices": []
        });
        return axum::response::Json(empty_res).into_response();
    }
    
    let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    let send_telemetry = schema.stream_telemetry;
    let start_time = std::time::Instant::now();

    let skip_brain = match &request.temporary_chat {
        Some(TemporaryChatMode::Strict) => true,
        _ => false,
    };

    let mut matched_tool = String::new();
    let mut jit_injected = false;
    let keep_alive_val = request.keep_alive;
    
    // We will modify the last message in the array if a skill is matched
    let mut augmented_messages = request.messages.clone();

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
                if let Some(last_msg) = augmented_messages.last_mut() {
                    last_msg.content = format!("{}\n\n{}", body, last_msg.content);
                }
                break; // Only inject one tool context for now to save space
            }
        }
    }



    // Serialize the entire message array to JSON to preserve full chat history
    let json_prompt = serde_json::to_string(&augmented_messages).unwrap_or_else(|_| last_message.clone());

    // Initial dispatch to see if it's a stream or an error
    let dispatch_result = state.dispatcher.dispatch_stream(&json_prompt, skip_brain).await;

    if request.stream {
        match dispatch_result {
            EngineResponse::TokenStream(initial_rx) => {
                let state_clone = state.clone();
                let temp_mode = request.temporary_chat.clone();
                let req_session_id = request.session_id.clone();
                
                let stream = async_stream::stream! {
                     let mut current_prompt = json_prompt.clone();
                     let mut total_generated = String::new();
                     let mut overall_token_count = 0;
                     let mut first_ttft_ms = 0;
                     let mut is_first_token = true;
                     let mut telemetry_sent = false;

                     // 🚀 YIELD MODEL HEADER METADATA EARLY (REAL-TIME UI UPDATE)
                     let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                     if send_telemetry && permission.model_header_info {
                         let loaded_models = generate_model_header_info();
                         let early_header_chunk = json!({
                             "id": "chatcmpl-123",
                             "object": "chat.completion.chunk",
                             "created": Utc::now().timestamp(),
                             "model": request.model.clone(),
                             "choices": [],
                             "usage": {
                                 "model_header_info": {
                                     "active_models": loaded_models
                                 }
                             }
                         });
                         yield Ok::<_, Infallible>(Event::default().data(early_header_chunk.to_string()));
                     }
                     
                     let mut rx = initial_rx;
                     
                     let mut max_iters = 3;
                    while max_iters > 0 {
                        max_iters -= 1;
                         let mut tool_executed = false;
                         
                         if !telemetry_sent {
                             telemetry_sent = true;
                             // We no longer stream __STEP_ markers as they corrupt standard OpenAI compatibility.
                         }
                         
                         while let Some(token) = rx.recv().await {
                            if token.trim() == "[DONE]" {
                                break;
                            }
                            
                            if token.contains("<TRIGGER:") && token.contains("</TRIGGER>") {
                                // Do not yield the raw `<TRIGGER` token to the content stream.
                                // Instead, we will construct the `tool_calls` block after parsing.

                                // Handle model stuttering by taking everything from the LAST <TRIGGER:
                                let trigger_start_idx = token.rfind("<TRIGGER:").unwrap_or(0);
                                let clean_token = &token[trigger_start_idx..];

                                
                                let (comp_type, comp_name, payload, execution_result) = {
                                    let header_end = clean_token.find('>').unwrap_or(0);
                                    let header = &clean_token[..header_end];
                                    let parts: Vec<&str> = header.trim_start_matches("<TRIGGER:").split(':').collect();
                                    
                                    let comp_type = if parts.len() >= 2 { parts[0] } else { "extension" };
                                    let comp_name = if parts.len() >= 2 { parts[1] } else { parts[0] };
                                    
                                    tracing::info!("🔍 [API] Single-Pass Intercepted Request for {} '{}'.", comp_type, comp_name);
                                    
                                    // Extract the JSON payload
                                    let json_start = header_end + 1;
                                    let json_end = clean_token.find("</TRIGGER>").unwrap_or(clean_token.len());
                                    let payload = clean_token[json_start..json_end].trim();
                                    
                                    tracing::info!("⚙️ [API] Extracted JSON Payload: {}", payload);
                                    
                                    let mut execution_result = String::new();
                                    
                                    {
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
                                    }

                                    (comp_type.to_string(), comp_name.to_string(), payload.to_string(), execution_result)
                                };

                                // Yield standard OpenAI tool_calls chunk before execution blocks
                                let tool_calls_chunk = json!({
                                    "id": "chatcmpl-123",
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
                                                    "name": comp_name,
                                                    "arguments": payload
                                                }
                                            }]
                                        }
                                    }]
                                });
                                yield Ok::<_, Infallible>(Event::default().data(tool_calls_chunk.to_string()));

                                // Yield the execution result as a tool status chunk (so UI knows it finished)
                                // Note: In strict OpenAI this isn't streamed to the user (it's added to history and the model continues),
                                // but for our interactive UI, we emit a special Cluaiz internal status block that the UI can catch.
                                let result_chunk = json!({
                                    "id": "chatcmpl-123",
                                    "object": "chat.completion.chunk",
                                    "created": Utc::now().timestamp(),
                                    "model": request.model.clone(),
                                    "choices": [{
                                        "index": 0,
                                        "delta": {
                                            "cluaiz_tool_result": {
                                                "id": format!("call_{}", comp_name),
                                                "result": execution_result.clone()
                                            }
                                        }
                                    }]
                                });
                                yield Ok::<_, Infallible>(Event::default().data(result_chunk.to_string()));
                                
                                // 🚀 SOVEREIGN KV-CACHE RESUME 
                                current_prompt = format!(
                                    "{}\n\n[PIVOT_CONTINUE]\n<result:{}:{}>\n{}\n</result>\nNow, provide the final conversational answer to the user based on the tool result above. Do NOT use any tools. Just answer the user directly.\n",
                                    current_prompt, comp_type, comp_name, execution_result
                                );


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
                        let mut usage_json = json!({
                            "completion_tokens": overall_token_count,
                            "total_tokens": overall_token_count,
                            "time_to_first_token_ms": first_ttft_ms,
                            "total_time_ms": total_time_ms,
                            "tokens_per_second": format!("{:.2}", tps).parse::<f64>().unwrap_or(0.0)
                        });

                        // Inject model_header_info if enabled in PermissionSchema
                        let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                        if permission.model_header_info {
                            let loaded_models = generate_model_header_info();
                            usage_json["model_header_info"] = json!({
                                "active_models": loaded_models
                            });
                        }

                        let telemetry_chunk = json!({
                            "id": "chatcmpl-123",
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [],
                            "usage": usage_json
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
                    
                    // 🛑 POST-GENERATION UNLOAD
                    if keep_alive_val == Some(0) {
                        tracing::info!("♻️ [Memory] Unloading model post-generation due to keep_alive: 0");
                        let _ = state_clone.dispatcher.unload_model().await;
                    }
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
            let mut usage_json = json!({
                "total_time_ms": total_time_ms
            });
            
            let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            if permission.model_header_info {
                let loaded_models = generate_model_header_info();
                usage_json["model_header_info"] = json!({
                    "active_models": loaded_models
                });
            }
            
            response["usage"] = usage_json;
        }

        // 🛑 POST-GENERATION UNLOAD
        if keep_alive_val == Some(0) {
            tracing::info!("♻️ [Memory] Unloading model post-generation (non-streaming) due to keep_alive: 0");
            let _ = state.dispatcher.unload_model().await;
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

