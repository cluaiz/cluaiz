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
    pub model: Option<String>,
    pub messages: Vec<ExternalMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temporary_chat: Option<TemporaryChatMode>,
    #[serde(default)]
    pub session_id: Option<String>,
    // Cluaiz Extension Parameters
    pub think_mode: Option<String>,
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
    // 🛡️ Model Registry & Dynamic Context Limit (Min 2k Floor)
    let dynamic_context_limit = engines::models::InstalledStateRegistry::load()
        .installed_models
        .get(request.model.as_deref().unwrap_or("default"))
        .and_then(|m| m.metadata.context_window.parse::<usize>().ok())
        .unwrap_or(2048)
        .max(2048);

    let validated_max_tokens = request.max_tokens.map(|t| t.min(dynamic_context_limit));
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
    let prompt_lower = last_message.flatten_to_string().await.to_lowercase();
    let mut matched_skills = Vec::new();
    if let Ok(router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.read() {
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
    }
    
    for skill_path in matched_skills {
        if let Some(body) = engines::neural_foundry::extract_skill_body(&skill_path) {
            if let Some(name) = std::path::Path::new(&skill_path).file_name() {
                matched_tool = name.to_string_lossy().to_string();
            }
            jit_injected = true;
            if let Some(last_msg) = augmented_messages.last_mut() {
                let prev_content = last_msg.content.flatten_to_string().await;
                last_msg.content = MessageContent::Text(format!("{}\n\n{}", body, prev_content));
            }
            break; // Only inject one tool context for now to save space
        }
    }

    // 🌡️ DYNAMIC RESPONSE LENGTH & TEMPERATURE CONSTRAINT
    let mut gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
    let mut applied_constraint: Option<String> = None;
    let mut applied_temp: Option<f64> = request.temperature.map(|t| t as f64);
    
    let map_val = gguf_meta.user_moved_flags.response_length.to_value();

    if let Some(payload_val) = &request.response_length {
        if let Some(payload_map) = payload_val.as_object() {
            // If UI sends a custom map, pick the first one as override
            if let Some((temp_str, constraint_val)) = payload_map.iter().next() {
                if let Ok(temp) = temp_str.parse::<f64>() {
                    applied_temp = Some(temp);
                    if let Some(c_str) = constraint_val.as_str() {
                        applied_constraint = Some(c_str.to_string());
                    }
                }
            }
        } else if let Some(mode_str) = payload_val.as_str() {
            // If UI sends a predefined mode as string, lookup from config dynamically
            if let Some(map_obj) = map_val.as_object() {
                for branch_name in &["think_on", "think_off"] {
                    if let Some(branch) = map_obj.get(*branch_name).and_then(|v| v.as_object()) {
                        if let Some(mode_obj) = branch.get(mode_str).and_then(|v| v.as_object()) {
                            if let Some((temp_str, constraint_val)) = mode_obj.iter().next() {
                                if let Ok(temp) = temp_str.parse::<f64>() {
                                    applied_temp = Some(temp);
                                    if let Some(c_str) = constraint_val.as_str() {
                                        applied_constraint = Some(c_str.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if applied_constraint.is_none() {
        let current_temp = applied_temp.unwrap_or(gguf_meta.samplers.temp as f64);
        let current_temp_str = current_temp.to_string();
        let current_temp_str_1 = format!("{:.1}", current_temp);

        if let Some(map_obj) = map_val.as_object() {
            let active_think_mode = request.think_mode.as_deref().unwrap_or(gguf_meta.user_moved_flags.think_mode.as_str());
            
            // If Auto, we DO NOT inject any default constraints. The AI handles it automatically.
            if !active_think_mode.eq_ignore_ascii_case("auto") {
                if let Some(t) = map_obj.get("type").and_then(|v| v.as_str()) {
                    if t == "predefined" {
                        let branch_key = if active_think_mode.eq_ignore_ascii_case("on") { "think_on" } else { "think_off" };
                        if let Some(branch) = map_obj.get(branch_key).and_then(|v| v.as_object()) {
                            for (_, temp_obj) in branch {
                                if let Some(temp_map) = temp_obj.as_object() {
                                    if let Some(constraint_val) = temp_map.get(&current_temp_str).or_else(|| temp_map.get(&current_temp_str_1)) {
                                        if let Some(constraint_str) = constraint_val.as_str() {
                                            applied_constraint = Some(constraint_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    } else if t == "custom" {
                        if let Some(constraint_val) = map_obj.get(&current_temp_str).or_else(|| map_obj.get(&current_temp_str_1)) {
                            if let Some(constraint_str) = constraint_val.as_str() {
                                applied_constraint = Some(constraint_str.to_string());
                            }
                        }
                    }
                } else {
                    if let Some(constraint_val) = map_obj.get(&current_temp_str).or_else(|| map_obj.get(&current_temp_str_1)) {
                        if let Some(constraint_str) = constraint_val.as_str() {
                            applied_constraint = Some(constraint_str.to_string());
                        }
                    }
                }
            } else {
                if let Some(constraint_val) = map_obj.get(&current_temp_str).or_else(|| map_obj.get(&current_temp_str_1)) {
                    if let Some(constraint_str) = constraint_val.as_str() {
                        applied_constraint = Some(constraint_str.to_string());
                    }
                }
            }
        }
    }

    // Apply strict payload overrides (Phase 3)
    if let Some(t) = request.temperature {
        gguf_meta.samplers.temp = t as f64;
    } else if let Some(temp) = applied_temp {
        gguf_meta.samplers.temp = temp;
    }

    if let Some(p) = request.top_p {
        gguf_meta.samplers.top_p = p as f64;
    }
    if let Some(k) = request.top_k {
        gguf_meta.samplers.top_k = k as usize;
    }
    if let Some(mp) = request.min_p {
        gguf_meta.samplers.min_p = mp as f64;
    }
    if let Some(pp) = request.presence_penalty {
        gguf_meta.samplers.presence_penalty = pp as f64;
    }
    if let Some(rp) = request.repetition_penalty {
        gguf_meta.samplers.repeat_penalty = rp as f64;
    }

    if let Some(think_mode) = &request.think_mode {
        gguf_meta.user_moved_flags.think_mode = think_mode.clone();
    }

    if let Some(constraint) = applied_constraint {
        if !constraint.is_empty() {
            tracing::info!("🌡️ [Prompt] Injecting response constraint.");
            if let Some(sys_msg) = augmented_messages.iter_mut().find(|m| m.role.to_lowercase() == "system") {
                let prev_sys = sys_msg.content.flatten_to_string().await;
                sys_msg.content = MessageContent::Text(format!("{}\n\n{}", prev_sys, constraint));
            } else {
                augmented_messages.insert(0, ExternalMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(constraint.to_string()),
                });
            }
        }
    }

    // Serialize the entire message array to JSON to preserve full chat history.
    // Packaging in-memory samplers into the prompt envelope eliminates disk race conditions and threads parameters directly into generation.
    let mut serialized_messages = Vec::new();
    for msg in &augmented_messages {
        let content_str = msg.content.flatten_to_string().await;
        serialized_messages.push(json!({
            "role": msg.role,
            "content": content_str
        }));
    }
    let payload_envelope = json!({
        "messages": serialized_messages,
        "samplers": {
            "temp": gguf_meta.samplers.temp,
            "top_p": gguf_meta.samplers.top_p,
            "top_k": gguf_meta.samplers.top_k,
            "min_p": gguf_meta.samplers.min_p,
            "presence_penalty": gguf_meta.samplers.presence_penalty,
            "repeat_penalty": gguf_meta.samplers.repeat_penalty
        },
        "think_mode": request.think_mode.as_deref().unwrap_or(gguf_meta.user_moved_flags.think_mode.as_str())
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
                                
                                // 🚀 SOVEREIGN KV-CACHE RESUME 
                                let pivot_envelope = json!({
                                    "pivot_prompt": format!(
                                        "<result:{}:{}>\n{}\n</result>\nNow, provide the final conversational answer to the user based on the tool result above. Do NOT use any tools. Just answer the user directly.\n",
                                        comp_type, comp_name, execution_result
                                    ),
                                    "samplers": {
                                        "temp": gguf_meta.samplers.temp,
                                        "top_p": gguf_meta.samplers.top_p,
                                        "top_k": gguf_meta.samplers.top_k,
                                        "min_p": gguf_meta.samplers.min_p,
                                        "presence_penalty": gguf_meta.samplers.presence_penalty,
                                        "repeat_penalty": gguf_meta.samplers.repeat_penalty
                                    },
                                    "think_mode": request.think_mode.as_deref().unwrap_or(gguf_meta.user_moved_flags.think_mode.as_str())
                                });
                                current_prompt = format!("[PIVOT_CONTINUE]{}", serde_json::to_string(&pivot_envelope).unwrap_or_default());


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
                    
                    // 🛑 POST-GENERATION UNLOAD / KEEP_ALIVE TIMING
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

