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

    // Send to Master Router
    let dispatch_result = state.dispatcher.dispatch_stream(&last_message, skip_brain).await;

    if request.stream {
        match dispatch_result {
            EngineResponse::TokenStream(mut rx) => {
                let state_clone = state.clone();
                let temp_mode = request.temporary_chat.clone();
                let req_session_id = request.session_id.clone();
                let stream = async_stream::stream! {
                    let mut first_token = true;
                    let mut ttft_ms = 0;
                    let mut token_count = 0;
                    let mut full_response = String::new();

                    while let Some(token) = rx.recv().await {
                        full_response.push_str(&token);
                        if first_token {
                            ttft_ms = start_time.elapsed().as_millis();
                            first_token = false;
                        }
                        token_count += 1;

                        let chunk = json!({
                            "id": "chatcmpl-123",
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [{"delta": {"content": token}}]
                        });
                        yield Ok::<_, Infallible>(Event::default().data(chunk.to_string()));
                    }

                    let total_time_ms = start_time.elapsed().as_millis();
                    let tps = if total_time_ms > 0 {
                        (token_count as f64 / (total_time_ms as f64 / 1000.0))
                    } else {
                        0.0
                    };

                    if send_telemetry {
                        let mut pulse_json = json!({});
                        if let Ok(lock) = cluaize_shared::hardware::telemetry::get_pulse().pulse.read() {
                            pulse_json = serde_json::to_value(&*lock).unwrap_or(json!({}));
                        }

                        let telemetry_chunk = json!({
                            "id": "chatcmpl-123",
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": request.model.clone(),
                            "choices": [],
                            "usage": {
                                "completion_tokens": token_count,
                                "total_tokens": token_count,
                                "time_to_first_token_ms": ttft_ms,
                                "total_time_ms": total_time_ms,
                                "tokens_per_second": format!("{:.2}", tps).parse::<f64>().unwrap_or(0.0),
                                "hardware_snapshot": pulse_json
                            }
                        });
                        yield Ok::<_, Infallible>(Event::default().data(telemetry_chunk.to_string()));
                    }

                    // 🧠 Save to Engine Brain
                    if let Ok(vec) = state_clone.embedding_dispatcher.dispatch_embedding(&full_response) {
                        if let Some(id) = req_session_id.clone() {
                            let _ = engines::memory::tensor_transducer::TensorTransducer::save_context(&id, &full_response, &vec);
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
                let _ = engines::memory::tensor_transducer::TensorTransducer::save_context(&id, &content, &vec);
            }
        }

        if send_telemetry {
            let total_time_ms = start_time.elapsed().as_millis();
            let mut pulse_json = json!({});
            if let Ok(lock) = cluaize_shared::hardware::telemetry::get_pulse().pulse.read() {
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

