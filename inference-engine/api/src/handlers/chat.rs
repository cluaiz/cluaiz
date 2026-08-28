use axum::{
    extract::{State},
    response::{Json, Sse, sse::Event, IntoResponse},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::{Arc, RwLock, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use crate::state::AppState;
use chrono::Utc;
use dispatcher::EngineResponse;

/// Thread-safe multi-stream controller signals
#[derive(Clone, Default)]
pub struct StreamSignals {
    pub cancel: Arc<AtomicBool>,
    pub skip_reasoning: Arc<AtomicBool>,
}

/// Global active streaming sessions registry (Keyed by unique `chatcmpl-...` stream_id)
pub static ACTIVE_STREAMS: LazyLock<RwLock<HashMap<String, StreamSignals>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Deserialize)]
pub struct StreamControlRequest {
    pub stream_id: String,
    pub reason: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TemporaryChatMode {
    Lite,
    Strict,
}

#[derive(Deserialize)]
pub struct ExternalChatRequest {
    pub model: Option<String>,
    pub messages: Vec<ExternalMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temporary_chat: Option<TemporaryChatMode>,
    #[serde(default)]
    pub session_id: Option<String>,
    // Cluaiz Extension & OpenAI Reasoning Parameters
    pub think_mode: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
    pub skip_reasoning: Option<bool>,
    pub response_length: Option<serde_json::Value>,
    pub keep_alive: Option<i32>,
    pub min_p: Option<f32>,
    pub repetition_penalty: Option<f32>,

    // Standard OpenAI Parameters
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub seed: Option<i64>,
    pub response_format: Option<serde_json::Value>,
    pub logit_bias: Option<std::collections::HashMap<String, f32>>,
    pub tools: Option<Vec<serde_json::Value>>,
    pub tool_choice: Option<serde_json::Value>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Array(Vec<ContentPart>),
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

impl MessageContent {
    pub async fn flatten_to_string(&self) -> String {
        match self {
            MessageContent::Text(text) => text.clone(),
            MessageContent::Array(parts) => {
                let mut combined = String::new();
                for part in parts {
                    match part {
                        ContentPart::Text { text } => {
                            combined.push_str(text);
                            combined.push('\n');
                        }
                        ContentPart::ImageUrl { image_url } => {
                            let local_path = match crate::url_resolver::resolve_to_local_file(&image_url.url).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve image URL: {}", e);
                                    image_url.url.clone()
                                }
                            };
                            combined.push_str(&format!("<cluaiz_media type=\"image\" url=\"{}\" />\n", local_path));
                        }
                        ContentPart::AudioUrl { audio_url } => {
                            let local_path = match crate::url_resolver::resolve_to_local_file(&audio_url.url).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve audio URL: {}", e);
                                    audio_url.url.clone()
                                }
                            };
                            combined.push_str(&format!("<cluaiz_media type=\"audio\" url=\"{}\" />\n", local_path));
                        }
                        ContentPart::InputAudio { input_audio } => {
                            let data_uri = format!("data:audio/{};base64,{}", input_audio.format, input_audio.data);
                            let local_path = match crate::url_resolver::resolve_to_local_file(&data_uri).await {
                                Ok(p) => p,
                                Err(e) => {
                                    tracing::error!("Failed to resolve input_audio: {}", e);
                                    data_uri
                                }
                            };
                            combined.push_str(&format!("<cluaiz_media type=\"audio\" url=\"{}\" />\n", local_path));
                        }
                    }
                }
                combined.trim_end().to_string()
            }
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: MediaUrlContent },
    #[serde(rename = "audio_url")]
    AudioUrl { audio_url: MediaUrlContent },
    #[serde(rename = "input_audio")]
    InputAudio { input_audio: InputAudioContent },
}

#[derive(Deserialize, Clone, Debug)]
pub struct InputAudioContent {
    pub data: String,
    #[serde(default = "default_audio_format")]
    pub format: String,
}

fn default_audio_format() -> String {
    "wav".to_string()
}

#[derive(Deserialize, Clone, Debug)]
pub struct MediaUrlContent {
    pub url: String,
}

#[derive(Deserialize, Clone)]
pub struct ExternalMessage {
    pub role: String,
    pub content: MessageContent,
}

// `ExternalMessage` intentionally does not derive Serialize here because its `content` field needs to be resolved asynchronously before serialization.

// ─── HELPER: Fetch Real Model Header Information ────────────
fn generate_model_header_info() -> Vec<Value> {
    let registry = cluaiz_shared::hardware::governor::HardwareGovernor::get_active_allocations();
    let mut loaded_models = Vec::new();
    let env = cluaiz_shared::environment::EnvironmentManager::current();
    let roots = vec![env.local_dir.join("models"), env.global_dir.join("models")];
    let categories = ["chat", "embedding", "vision", "audio", "code"];

    for info in registry {
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
                                    if let Ok((meta, _tensors, _count)) = engines::models::GgufProber::probe(&p) {
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
    let request_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    
    // 🛡️ API Boundary Input Validation: max_tokens must be >= 1 if provided
    if let Some(tokens) = request.max_tokens {
        if tokens == 0 {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(json!({
                    "error": {
                        "message": "max_tokens must be greater than or equal to 1",
                        "type": "invalid_request_error",
                        "param": "max_tokens",
                        "code": "parameter_out_of_range"
                    }
                })),
            ).into_response();
        }
    }

    let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
    
    // Check if empty content should be prevented
    if last_message.flatten_to_string().await.is_empty() && request.keep_alive == Some(0) {
        tracing::info!("♻️ [Memory] Instant model unload requested via keep_alive: 0");
        let _ = state.dispatcher.unload_model().await;
        let empty_res = json!({
            "id": request_id.clone(),
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": request.model.clone().unwrap_or_else(|| "default-model".to_string()),
            "choices": []
        });
        return axum::response::Json(empty_res).into_response();
    }
    
    let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    let send_telemetry = schema.stream_telemetry;
    let start_time = std::time::Instant::now();

    // 🛡️ STRICT PRE-FLIGHT TASK GUARDRAIL
    // Ensure the currently active chat or vision slot actually supports chat capabilities
    // (Prevent loading pure embedding/STT models into chat API and causing GPU crash)
    let mut target_slot = "chat_slot";
    
    // Simple heuristic: If image url is present, we might want vision_slot. 
    // In actual implementation, we might check which slot has the model.
    let has_image = request.messages.iter().any(|m| {
        match &m.content {
            crate::handlers::chat::MessageContent::Array(parts) => {
                parts.iter().any(|p| matches!(p, crate::handlers::chat::ContentPart::ImageUrl { .. }))
            },
            _ => false
        }
    });

    if has_image && schema.active_slots.contains_key("vision_slot") {
        target_slot = "vision_slot";
    }

    if let Err(err_response) = crate::utils::slots::require_capability(
        &schema, 
        target_slot, 
        &["chat-completion", "text-generation", "vision-chat", "multimodal-vision"]
    ) {
        tracing::error!("Blocked chat request: Active slot '{}' does not support chat completions.", target_slot);
        return err_response.into_response();
    }

    let mut active_model_path = crate::utils::slots::resolve_model_path(&schema, target_slot);
    let mut resolved_model_name = "default-system-model".to_string();

    if let Some(ref m_id) = request.model {
        if !m_id.trim().is_empty() {
            if let Some(explicit_path) = crate::utils::slots::resolve_model_by_id(m_id) {
                tracing::info!("🤖 [API] Model override requested. Resolved '{}' to {:?}", m_id, explicit_path);
                active_model_path = Some(explicit_path);
                resolved_model_name = m_id.clone();
            } else {
                tracing::warn!("⚠️ [API] Requested model '{}' not found in registry. Falling back to default slot model.", m_id);
            }
        }
    } else {
        tracing::info!("🤖 [API] No model specified (null). Falling back to default '{}' model.", target_slot);
    }
    
    // Ensure we actually have a path to load
    if active_model_path.is_none() {
        let err_res = json!({
            "error": {
                "message": format!("No model is currently loaded in slot '{}' and no valid override was provided.", target_slot),
                "type": "invalid_request_error",
                "code": "model_not_found"
            }
        });
        return axum::response::Json(err_res).into_response();
    }

    // 🛡️ Dynamic Context Limit: Resolved from InstalledStateRegistry or active slot (Min 2k Floor)
    let installed_registry = engines::models::InstalledStateRegistry::load();
    let dynamic_context_limit = installed_registry
        .installed_models
        .get(&resolved_model_name)
        .or_else(|| {
            schema.active_slots.get(target_slot)
                .and_then(|slot| slot.model_id.as_deref())
                .and_then(|id| installed_registry.installed_models.get(id))
        })
        .or_else(|| {
            let clean = resolved_model_name.trim_end_matches(".gguf");
            installed_registry.installed_models.get(clean)
        })
        .and_then(|m| m.metadata.context_window.parse::<usize>().ok())
        .unwrap_or(2048)
        .max(2048);

    let validated_max_tokens = request.max_tokens.map(|t| t.min(dynamic_context_limit));

    let skip_brain = match &request.temporary_chat {
        Some(TemporaryChatMode::Strict) => true,
        _ => false,
    };

    let mut matched_tool = String::new();
    let mut prefix_caching_injected = false;
    let keep_alive_val = request.keep_alive;
    
    // We will modify the last message in the array if a skill is matched
    let mut augmented_messages = request.messages.clone();

    // 🚀 RESOLVE ACTIVE SESSION TOOLS & SEMANTIC ROUTING (Turn Lifecycle & Skills Injection)
    let mut active_skill_bodies = Vec::new();

    // 1. Check Session Tools bound to session_id
    if let Some(ref sid) = request.session_id {
        let session_tools = engines::tools::SessionToolManager::get_session_tools(sid);
        for tool_binding in session_tools {
            if let Some(body) = engines::tools::ToolsEngine::get_skill_instructions(&tool_binding.id) {
                active_skill_bodies.push(body);
            }
        }
    }

    // 2. Also check one-off request.tools payload (Ephemeral tools)
    if let Some(ref tools_vec) = request.tools {
        for tool_val in tools_vec {
            if let Some(tool_id) = tool_val.get("id").and_then(|v| v.as_str()) {
                if let Some(body) = engines::tools::ToolsEngine::get_skill_instructions(tool_id) {
                    if !active_skill_bodies.contains(&body) {
                        active_skill_bodies.push(body);
                    }
                }
            }
        }
    }

    // 3. Fallback to semantic triggers matching on prompt
    let prompt_lower = last_message.flatten_to_string().await.to_lowercase();
    let matched_skills = engines::tools::ToolsEngine::match_skills(&prompt_lower);
    for skill_id in matched_skills {
        if let Some(body) = engines::tools::ToolsEngine::get_skill_instructions(&skill_id) {
            if !active_skill_bodies.contains(&body) {
                active_skill_bodies.push(body);
                break; // Limit semantic trigger match to 1
            }
        }
    }

    // 🧠 Context Window & Tool Instruction Budgeting Safety Cap
    let gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
    let n_ctx_limit = (gguf_meta.hardware_and_execution.n_ctx as usize).max(512);

    let max_tool_chars = (n_ctx_limit * 4 * 35) / 100; // max 35% of context window for tool prompts
    let mut total_chars = 0;
    let mut budgeted_skill_bodies = Vec::new();
    for body in active_skill_bodies {
        if total_chars + body.len() <= max_tool_chars || budgeted_skill_bodies.is_empty() {
            total_chars += body.len();
            budgeted_skill_bodies.push(body);
        } else {
            tracing::warn!("⚠️ [ChatHandler] Skill prompt truncated to prevent Context Window overflow (budget limit: {} chars)", max_tool_chars);
            break;
        }
    }

    // Inject budgeted active tool instructions into the last user prompt context
    if !budgeted_skill_bodies.is_empty() {
        if let Some(last_msg) = augmented_messages.last_mut() {
            let prev_content = last_msg.content.flatten_to_string().await;
            let combined_instructions = budgeted_skill_bodies.join("\n\n---\n\n");
            last_msg.content = MessageContent::Text(format!("{}\n\n{}", combined_instructions, prev_content));
        }
    }

    // 🧠 REASONING & THINKING BUDGET RESOLUTION (OpenAI reasoning_effort + Cluaiz think_mode)
    let active_think_mode_owned = request.reasoning_effort.as_deref()
        .map(|re| re.to_string())
        .or_else(|| {
            request.think_mode.as_ref().map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else if let Some(b) = v.as_bool() {
                    if b { "high".to_string() } else { "off".to_string() }
                } else if let Some(n) = v.as_i64() {
                    n.to_string()
                } else if let Some(n) = v.as_u64() {
                    n.to_string()
                } else {
                    "auto".to_string()
                }
            })
        })
        .unwrap_or_else(|| gguf_meta.user_moved_flags.think_mode.clone());

    // Mathematical clamping of custom integer budgets against n_ctx and max_tokens
    let normalized_think_mode = match active_think_mode_owned.to_lowercase().as_str() {
        "off" | "false" | "0" | "minimal" => "off".to_string(),
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" | "on" | "max" => "high".to_string(),
        "auto" => "auto".to_string(),
        custom_str => {
            if let Ok(custom_budget) = custom_str.parse::<usize>() {
                if custom_budget == 0 {
                    "off".to_string()
                } else {
                    let max_tok = validated_max_tokens.unwrap_or(2048);
                    let clamped = custom_budget
                        .min(n_ctx_limit)
                        .min(max_tok.saturating_sub(32).max(1));
                    clamped.to_string()
                }
            } else {
                "auto".to_string()
            }
        }
    };
    let active_think_mode = normalized_think_mode;

    let active_response_length = request.response_length.as_ref().map(|v| {
        if let Some(s) = v.as_str() {
            s.to_string()
        } else if let Some(n) = v.as_i64() {
            n.to_string()
        } else if let Some(n) = v.as_u64() {
            n.to_string()
        } else {
            "auto".to_string()
        }
    }).unwrap_or_else(|| gguf_meta.user_moved_flags.response_length.clone());

    // 🚀 ZERO-DISK CONCURRENCY: Direct In-Memory Payload & Sampler Dispatch
    // Packaging in-memory samplers into the prompt envelope eliminates disk race conditions and threads parameters directly into generation.
    let mut serialized_messages = Vec::new();
    let mut system_prompt_chars = 0;
    let mut history_chars = 0;
    let mut user_prompt_chars = 0;
    let total_msgs = augmented_messages.len();

    for (i, msg) in augmented_messages.iter().enumerate() {
        let content_str = msg.content.flatten_to_string().await;
        let c_len = content_str.len();
        if msg.role.eq_ignore_ascii_case("system") {
            system_prompt_chars += c_len;
        } else if i == total_msgs.saturating_sub(1) && msg.role.eq_ignore_ascii_case("user") {
            user_prompt_chars += c_len;
        } else {
            history_chars += c_len;
        }
        serialized_messages.push(json!({
            "role": msg.role,
            "content": content_str
        }));
    }
    let effective_temp = request.temperature.map(|t| t as f64).unwrap_or(gguf_meta.samplers.temp);
    let effective_top_p = request.top_p.map(|p| p as f64).unwrap_or(gguf_meta.samplers.top_p);
    let effective_top_k = request.top_k.map(|k| k as usize).unwrap_or(gguf_meta.samplers.top_k);
    let effective_min_p = request.min_p.map(|m| m as f64).unwrap_or(gguf_meta.samplers.min_p);
    let effective_presence = request.presence_penalty.map(|p| p as f64).unwrap_or(gguf_meta.samplers.presence_penalty);
    let effective_frequency = request.frequency_penalty.map(|f| f as f64).unwrap_or(gguf_meta.samplers.frequency_penalty);
    let effective_repeat = request.repetition_penalty.map(|r| r as f64).unwrap_or(gguf_meta.samplers.repeat_penalty);
    let effective_seed = request.seed.or(gguf_meta.samplers.seed.map(|s| s as i64));

    let payload_envelope = json!({
        "messages": serialized_messages,
        "samplers": {
            "temp": effective_temp,
            "top_p": effective_top_p,
            "top_k": effective_top_k,
            "min_p": effective_min_p,
            "presence_penalty": effective_presence,
            "frequency_penalty": effective_frequency,
            "repeat_penalty": effective_repeat,
            "seed": effective_seed
        },
        "think_mode": &active_think_mode,
        "response_length": &active_response_length
    });
    let json_prompt = serde_json::to_string(&payload_envelope).unwrap_or_else(|_| "{}".to_string());
    // Initial dispatch to see if it's a stream or an error
    let dispatch_result = state.dispatcher.dispatch_stream(&json_prompt, skip_brain, active_model_path.clone(), validated_max_tokens).await;

    if request.stream {

        match dispatch_result {
            EngineResponse::TokenStream(initial_rx) => {
                let state_clone = state.clone();
                let temp_mode = request.temporary_chat.clone();
                let req_session_id = request.session_id.clone();
                let req_id_stream = request_id.clone();
                let active_think_mode_stream = active_think_mode.clone();
                let active_response_length_stream = active_response_length.clone();
                
                // 🛡️ Multi-Tenant Stream Registry Registration
                let initial_skip_reasoning = request.skip_reasoning == Some(true) || active_think_mode == "off";
                if let Ok(mut lock) = ACTIVE_STREAMS.write() {
                    lock.insert(
                        req_id_stream.clone(),
                        StreamSignals {
                            cancel: Arc::new(AtomicBool::new(false)),
                            skip_reasoning: Arc::new(AtomicBool::new(initial_skip_reasoning)),
                        }
                    );
                }

                let stream = async_stream::stream! {
                     let mut current_prompt = json_prompt.clone();
                      let mut total_generated = String::new();
                      let mut overall_token_count = 0;
                      let mut reasoning_tokens_count = 0usize;
                      let mut in_think_block = false;
                      let mut first_ttft_ms = 0;
                      let mut is_first_token = true;
                      let mut telemetry_sent = false;

                     // 🚀 YIELD MODEL HEADER METADATA EARLY (REAL-TIME UI UPDATE)
                     let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                     if send_telemetry && permission.model_header_info {
                         let loaded_models = generate_model_header_info();
                         let early_header_chunk = json!({
                             "id": req_id_stream.clone(),
                             "object": "chat.completion.chunk",
                             "created": Utc::now().timestamp(),
                             "model": resolved_model_name.clone(),
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
                     let active_tools_list = if let Some(ref sid) = req_session_id {
                         crate::handlers::session_tools::get_active_tool_ids_for_session(sid)
                     } else {
                         Vec::new()
                     };

                     let context_snap = cluaiz_shared::telemetry::ContextTracker::build_snapshot(
                         validated_max_tokens.unwrap_or(32768),
                         &current_prompt,
                         "System",
                         &[],
                         &[],
                         &[],
                         &std::collections::HashMap::new(),
                         4096,
                         32,
                         32,
                         128,
                         2,
                         450,
                     );

                     let early_context_chunk = json!({
                         "id": req_id_stream.clone(),
                         "object": "chat.completion.chunk",
                         "created": Utc::now().timestamp(),
                         "model": resolved_model_name.clone(),
                         "choices": [],
                         "usage": {
                             "context_telemetry": context_snap,
                             "active_session_tools": active_tools_list
                         }
                     });
                     yield Ok::<_, Infallible>(Event::default().data(early_context_chunk.to_string()));
                     
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
                            if should_skip && in_think_block {
                                tracing::info!("⏩ [StreamControl] Skipping reasoning tokens for stream '{}'.", req_id_stream);
                                in_think_block = false;
                                let skip_chunk = json!({
                                    "id": req_id_stream.clone(),
                                    "object": "chat.completion.chunk",
                                    "created": Utc::now().timestamp(),
                                    "model": resolved_model_name.clone(),
                                    "choices": [{"delta": {"content": "\n</think>\n\n"}}]
                                });
                                yield Ok::<_, Infallible>(Event::default().data(skip_chunk.to_string()));
                            }

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
                                    
                                    let comp_type = if parts.len() >= 2 { parts[0] } else { "plugin" };
                                    let comp_name = if parts.len() >= 2 { parts[1] } else { parts[0] };
                                    
                                    tracing::info!("🔍 [API] Single-Pass Intercepted Request for {} '{}'.", comp_type, comp_name);
                                    
                                    // Extract the JSON payload
                                    let json_start = header_end + 1;
                                    let json_end = clean_token.find("</TRIGGER>").unwrap_or(clean_token.len());
                                    let payload = clean_token[json_start..json_end].trim();
                                    
                                    tracing::info!("⚙️ [API] Extracted JSON Payload: {}", payload);
                                    
                                    let mut execution_result = String::new();
                                    
                                    match engines::tools::ToolsEngine::execute_plugin_by_name(comp_name, payload.as_bytes()) {
                                        Ok(bytes) => {
                                            execution_result = String::from_utf8_lossy(&bytes).to_string();
                                            tracing::info!("✅ [API] Tool execution completed. Result length: {}", execution_result.len());
                                        },
                                        Err(e) => {
                                            execution_result = format!("Error executing {}: {}", comp_name, e);
                                            tracing::error!("❌ [API] Failed to execute tool: {}", e);
                                        }
                                    }

                                    (comp_type.to_string(), comp_name.to_string(), payload.to_string(), execution_result)
                                };

                                // Yield standard OpenAI tool_calls chunk before execution blocks
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
                                    "id": req_id_stream.clone(),
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
                                
                                // 🔄 Dynamic KV-Cache & Tool Result Resume (Preserving User Context)
                                let user_query = last_message.flatten_to_string().await;
                                let pivot_envelope = json!({
                                    "pivot_prompt": format!(
                                        "<user_query>\n{}\n</user_query>\n<result:{}:{}>\n{}\n</result>\nNow, provide the final conversational answer to the user based on the user query and tool result above. Do NOT use any tools. Just answer the user directly.\n",
                                        user_query, comp_type, comp_name, execution_result
                                    ),
                                    "samplers": {
                                        "temp": effective_temp,
                                        "top_p": effective_top_p,
                                        "top_k": effective_top_k,
                                        "min_p": effective_min_p,
                                        "presence_penalty": effective_presence,
                                        "frequency_penalty": effective_frequency,
                                        "repeat_penalty": effective_repeat,
                                        "seed": effective_seed
                                    },
                                    "think_mode": &active_think_mode_stream,
                                    "response_length": &active_response_length_stream
                                });
                                current_prompt = format!("[PIVOT_CONTINUE]{}", serde_json::to_string(&pivot_envelope).unwrap_or_default());


                                tool_executed = true;
                                break;
                            }
                            
                            // Normal Token Yielding & Reasoning Detection
                            if token.contains("<think") || token.contains("<|thought") || token.contains("<thought") {
                                in_think_block = true;
                            }
                            if in_think_block {
                                reasoning_tokens_count += 1;
                            }
                            if token.contains("</think>") || token.contains("<channel|>") || token.contains("</thought>") || token.contains("</|thought|>") {
                                in_think_block = false;
                            }

                            total_generated.push_str(&token);
                            overall_token_count += 1;
                            
                            if is_first_token {
                                first_ttft_ms = start_time.elapsed().as_millis();
                                is_first_token = false;
                            }
                            
                            let chunk = json!({
                                "id": req_id_stream.clone(),
                                "object": "chat.completion.chunk",
                                "created": Utc::now().timestamp(),
                                "model": resolved_model_name.clone(),
                                "choices": [{"delta": {"content": token}}]
                            });
                            yield Ok::<_, Infallible>(Event::default().data(chunk.to_string()));
                        }
                        
                        if tool_executed {
                            tracing::info!("🔄 [API] Resuming generation with tool result (Single-Pass)...");
                            let new_dispatch = state_clone.dispatcher.dispatch_stream(&current_prompt, skip_brain, active_model_path.clone(), validated_max_tokens).await;
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
                    let decode_time_ms = total_time_ms.saturating_sub(first_ttft_ms);
                    let tps = if decode_time_ms > 30 {
                        (overall_token_count as f64) / (decode_time_ms as f64 / 1000.0)
                    } else if total_time_ms > 0 {
                        (overall_token_count as f64) / (total_time_ms as f64 / 1000.0)
                    } else {
                        0.0
                    };

                    if send_telemetry {
                        let prompt_tok_est = if user_prompt_chars + history_chars + system_prompt_chars > 0 {
                            ((user_prompt_chars + history_chars + system_prompt_chars) / 4).max(1)
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
                        let active_ids = req_session_id.as_deref()
                            .map(engines::tools::ToolsEngine::get_active_tool_ids_for_session)
                            .unwrap_or_default();
                        let ctx_telemetry = engines::tools::ToolsEngine::compute_telemetry(
                            &resolved_model_name,
                            req_session_id.as_deref().unwrap_or("ephemeral"),
                            &active_ids,
                            user_prompt_chars,
                            history_chars,
                            system_prompt_chars,
                            overall_token_count,
                        );
                        usage_json["context_telemetry"] = serde_json::to_value(ctx_telemetry).unwrap_or_default();

                        // Inject model_header_info if enabled in PermissionSchema
                        let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                        if permission.model_header_info {
                            let loaded_models = generate_model_header_info();
                            usage_json["model_header_info"] = json!({
                                "active_models": loaded_models
                            });
                        }

                        let telemetry_chunk = json!({
                            "id": req_id_stream.clone(),
                            "object": "chat.completion.chunk",
                            "created": Utc::now().timestamp(),
                            "model": resolved_model_name.clone(),
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
                
                return Sse::new(stream).into_response();
            }
            EngineResponse::FinalResult(res) => {
                let chunk = json!({
                    "id": request_id.clone(),
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
            "id": request_id.clone(),
            "object": "chat.completion",
            "created": Utc::now().timestamp(),
            "model": resolved_model_name.clone(),
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
            let comp_tok_est = content.split_whitespace().count().max(1);
            let prompt_tok_est = if user_prompt_chars + history_chars + system_prompt_chars > 0 {
                ((user_prompt_chars + history_chars + system_prompt_chars) / 4).max(1)
            } else {
                0
            };
            let mut usage_json = json!({
                "prompt_tokens": prompt_tok_est,
                "completion_tokens": comp_tok_est,
                "total_tokens": comp_tok_est + prompt_tok_est,
                "completion_tokens_details": {
                    "reasoning_tokens": 0
                },
                "total_time_ms": total_time_ms
            });
            
            // 🌐 Real-Time Context & Memory Telemetry Breakdown
            let active_ids = request.session_id.as_deref()
                .map(engines::tools::ToolsEngine::get_active_tool_ids_for_session)
                .unwrap_or_default();
            let ctx_telemetry = engines::tools::ToolsEngine::compute_telemetry(
                &resolved_model_name,
                request.session_id.as_deref().unwrap_or("ephemeral"),
                &active_ids,
                user_prompt_chars,
                history_chars,
                system_prompt_chars,
                comp_tok_est,
            );
            usage_json["context_telemetry"] = serde_json::to_value(ctx_telemetry).unwrap_or_default();
            
            let permission = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            if permission.model_header_info {
                let loaded_models = generate_model_header_info();
                usage_json["model_header_info"] = json!({
                    "active_models": loaded_models
                });
            }
            
            response["usage"] = usage_json;
        }

        // ⏳ Decrement session tool turns & purge expired ephemeral tools
        if let Some(ref sid) = request.session_id {
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

        return Json(response).into_response();
    }
}

// ─── POST /v1/chat/cancel (Cancel Active Stream) ──────────────────────
pub async fn cancel_chat_stream(
    Json(payload): Json<StreamControlRequest>,
) -> axum::response::Response {
    let signal_found = if let Ok(lock) = ACTIVE_STREAMS.read() {
        if let Some(entry) = lock.get(&payload.stream_id) {
            entry.cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    } else {
        false
    };

    if signal_found {
        // 🛑 Dual-Layer Signal: Trigger Deep Hardware Native C++ Llama Engine Interrupt
        cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(true, std::sync::atomic::Ordering::SeqCst);
        tracing::info!("🛑 [StreamControl] Deep cancel signal dispatched for stream '{}'.", payload.stream_id);
        axum::Json(json!({
            "status": "cancelled",
            "stream_id": payload.stream_id,
            "message": "Stream cancellation signal dispatched successfully."
        })).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(json!({
                "error": {
                    "message": format!("Active stream '{}' not found or already completed.", payload.stream_id),
                    "type": "invalid_request_error",
                    "code": "stream_not_found"
                }
            }))
        ).into_response()
    }
}

// ─── POST /v1/chat/skip-reasoning (Fast-Forward Thinking) ─────────────
pub async fn skip_chat_reasoning(
    Json(payload): Json<StreamControlRequest>,
) -> axum::response::Response {
    let signal_found = if let Ok(lock) = ACTIVE_STREAMS.read() {
        if let Some(entry) = lock.get(&payload.stream_id) {
            entry.skip_reasoning.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    } else {
        false
    };

    if signal_found {
        // ⏩ Dual-Layer Signal: Trigger Deep Hardware Native C++ Llama Reasoning Exit
        cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.store(true, std::sync::atomic::Ordering::SeqCst);
        tracing::info!("⏩ [StreamControl] Deep skip-reasoning signal dispatched for stream '{}'.", payload.stream_id);
        axum::Json(json!({
            "status": "skipped",
            "stream_id": payload.stream_id,
            "message": "Skip reasoning signal dispatched successfully."
        })).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(json!({
                "error": {
                    "message": format!("Active stream '{}' not found or already completed.", payload.stream_id),
                    "type": "invalid_request_error",
                    "code": "stream_not_found"
                }
            }))
        ).into_response()
    }
}
