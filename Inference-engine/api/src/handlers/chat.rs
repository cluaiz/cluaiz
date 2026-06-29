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

    // Initial dispatch to see if it's a stream or an error
    let dispatch_result = state.dispatcher.dispatch_stream(&last_message, skip_brain).await;

    if request.stream {
        match dispatch_result {
            EngineResponse::TokenStream(initial_rx) => {
                let state_clone = state.clone();
                let temp_mode = request.temporary_chat.clone();
                let req_session_id = request.session_id.clone();
                
                let stream = async_stream::stream! {
                    let mut current_prompt = last_message.clone();
                    let mut total_generated = String::new();
                    let mut overall_token_count = 0;
                    let mut first_ttft_ms = 0;
                    let mut is_first_token = true;
                    
                    let mut rx = initial_rx;
                    
                    loop {
                        let mut triggered = false;
                        
                        while let Some(token) = rx.recv().await {
                            // [SAFETY]: Two-Step Discovery Interceptor
                            // This block intercepts special `<TRIGGER:X:Y>` tokens emitted natively by the 
                            // `dispatcher` C-FFI loop. When intercepted, it dynamically loads the SKILL schema 
                            // from the MasterRegistry and injects it into the prompt without breaking the streaming loop.
                            if token.starts_with("<TRIGGER:") && token.ends_with(">") {
                                let parts: Vec<&str> = token.trim_matches(|c| c == '<' || c == '>').split(':').collect();
                                if parts.len() == 3 {
                                    let comp_type = parts[1];
                                    let comp_name = parts[2];
                                    
                                    tracing::info!("🔍 [API] Intercepted Request for {} '{}'. Fetching schema...", comp_type, comp_name);
                                    
                                    // Look up Master Registry
                                    let mut injection = String::new();
                                    if let Ok(registry) = MasterRegistry::load() {
                                        let entry_opt = if comp_type == "extension" {
                                            registry.extensions.get(comp_name)
                                        } else {
                                            registry.plugins.get(comp_name)
                                        };
                                        
                                        if let Some(entry) = entry_opt {
                                            let domain_path = EnvironmentManager::current().global_dir.join(&entry.domain);
                                            let fallback = if comp_type == "extension" {
                                                "manifest-extension.yaml"
                                            } else if comp_type == "plugin" {
                                                "manifest-plugin.yaml"
                                            } else {
                                                "manifest-mcp.yaml"
                                            };
                                            
                                            let manifest_path = domain_path.join(fallback);
                                            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                                                injection.push_str(&content);
                                                tracing::info!("✅ [API] Manifest schema injected for {}", comp_name);
                                            } else {
                                                tracing::warn!("⚠️ [API] Missing manifest {} for {}", fallback, comp_name);
                                            }
                                            
                                            let skill_path = domain_path.join("SKILL.md");
                                            if skill_path.exists() {
                                                if let Ok(skill_content) = std::fs::read_to_string(&skill_path) {
                                                    injection.push_str("\n\n--- AI SKILL MANUAL ---\n\n");
                                                    injection.push_str(&skill_content);
                                                    tracing::info!("✅ [API] SKILL.md appended for {}", comp_name);
                                                }
                                            }

                                            let permissions = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                            if let Some(gen_model) = permissions.get_active_chat_model() {
                                                let gen_model_safe = gen_model.replace(":", "-");
                                                let cache_dir = domain_path.join(".cache");
                                                if !cache_dir.exists() {
                                                    let _ = std::fs::create_dir_all(&cache_dir);
                                                }
                                                let kv_cache_path = cache_dir.join(format!("{}.kvcache.bin", gen_model_safe));
                                                if !kv_cache_path.exists() {
                                                    let roster = engines::models::registry::CoreRoster::load_roster();
                                                    if let Some(model_manifest) = roster.iter().find(|m| m.id == gen_model) {
                                                        if let Some(local_path) = &model_manifest.local_path {
                                                            let local_path_buf = std::path::PathBuf::from(local_path);
                                                            let mut backend_name = "llama".to_string();
                                                            if let Some(parent) = local_path_buf.parent() {
                                                                let dna_path = parent.join("structural_dna.json");
                                                                if let Ok(content) = std::fs::read_to_string(&dna_path) {
                                                                    if content.contains("\"RuntimeA\"") || content.contains("\"candlenative\"") {
                                                                        backend_name = "onnx".to_string();
                                                                    }
                                                                }
                                                            }
                                                            let rt = tokio::runtime::Handle::current();
                                                            let expanded_ctx = (injection.len() / 3 + 256) as usize;
                                                            tracing::info!("⏳ [API] Triggering Agentic Pause Dual-Cache Compilation for {}", comp_name);
                                                            engines::api::router::agentic_pause_compile_cache(
                                                                &rt,
                                                                kv_cache_path,
                                                                local_path_buf,
                                                                injection.clone(),
                                                                expanded_ctx,
                                                                backend_name
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Update Prompt Context for Resume
                                    current_prompt = format!(
                                        "{}{}\n\n[SYSTEM INJECTION: TOOL SCHEMA FOR {}]\n{}\n[SYSTEM: RESUME GENERATION]\n", 
                                        current_prompt, total_generated, comp_name, injection
                                    );
                                    
                                    triggered = true;
                                    break; // Break the token reception loop to re-invoke dispatch_stream
                                }
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
                        
                        if triggered {
                            // Re-invoke dispatcher with the new prompt
                            tracing::info!("🔄 [API] Resuming generation after injection...");
                            let new_dispatch = state_clone.dispatcher.dispatch_stream(&current_prompt, skip_brain).await;
                            if let EngineResponse::TokenStream(new_rx) = new_dispatch {
                                rx = new_rx;
                                continue;
                            } else {
                                break;
                            }
                        } else {
                            break; // LLM naturally finished
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

