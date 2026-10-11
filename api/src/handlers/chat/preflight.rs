use axum::response::IntoResponse;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use crate::state::AppState;
use super::types::{ExternalChatRequest, MessageContent, TemporaryChatMode};

pub struct PreparedChatContext {
    pub request_id: String,
    pub target_slot: String,
    pub resolved_model_name: String,
    pub active_model_path: Option<PathBuf>,
    pub dynamic_context_limit: usize,
    pub n_ctx_limit: usize,
    pub chat_tmpl: Option<String>,
    pub dyn_think_start: Option<String>,
    pub dyn_think_end: Option<String>,
    pub prompt_starts_in_think: bool,
    pub active_think_mode: String,
    pub active_reasoning_effort: String,
    pub validated_max_tokens: Option<usize>,
    pub skip_brain: bool,
    /// Request sampler values (llama.cpp names, null = not sent): the engine falls back to
    /// user settings, then model defaults, then llama.cpp defaults
    pub samplers: serde_json::Value,
    pub system_prompt_chars: usize,
    pub history_chars: usize,
    pub user_prompt_chars: usize,
    pub active_tool_ids: Vec<String>,
    pub json_prompt: String,
    pub initial_user_query: String,
    pub send_telemetry: bool,
    /// When the model was ready (after any load): TTFT and total time count from here
    pub start_time: std::time::Instant,
    /// Time spent loading the model for this request (0 when it was already loaded)
    pub model_load_ms: u128,
}

/// Error reply in the shape the client asked for (SSE chunk + [DONE] when streaming, JSON otherwise).
fn error_response(request: &ExternalChatRequest, request_id: &str, message: String, error_type: &str, code: Option<&str>) -> axum::response::Response {
    if request.stream {
        let err_id = request_id.to_string();
        let stream = async_stream::stream! {
            let err_chunk = json!({
                "id": err_id,
                "choices": [{"delta": {"content": format!("Error: {}", message)}}]
            });
            yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(err_chunk.to_string()));
            yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data("[DONE]"));
        };
        return axum::response::sse::Sse::new(stream).into_response();
    }
    axum::response::Json(json!({
        "error": { "message": message, "type": error_type, "code": code }
    })).into_response()
}

impl PreparedChatContext {
    pub async fn prepare(
        state: &Arc<AppState>,
        request: &ExternalChatRequest,
        request_id: &str,
    ) -> Result<Self, axum::response::Response> {
        let schema = cluaiz_security::PermissionSchema::load();
        let send_telemetry = schema.stream_telemetry;

        // 🛡️ STRICT PRE-FLIGHT TASK GUARDRAIL
        // Check if multimodal image exists to route to vision slot
        let mut target_slot = "chat_slot";
        let has_image = request.messages.iter().any(|m| {
            match &m.content {
                MessageContent::Array(parts) => {
                    parts.iter().any(|p| matches!(p, super::types::ContentPart::ImageUrl { .. }))
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
            return Err(err_response);
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
        
        // Ensure we actually have a path to load (Auto-heal fallback)
        if active_model_path.is_none() {
            let installed = cluaiz_models::registry::InstalledStateRegistry::load();
            for (id, entry) in &installed.installed_models {
                if entry.category == "chat" {
                    if let Some(p) = crate::utils::slots::resolve_model_by_id(id) {
                        tracing::info!("🔄 [API] Missing slot model auto-healed fallback to installed '{}' ({:?})", id, p);
                        active_model_path = Some(p);
                        resolved_model_name = id.clone();
                        break;
                    }
                }
            }
        }

        if active_model_path.is_none() {
            let err_msg = format!("No model is currently loaded in slot '{}' and no valid override was provided.", target_slot);
            return Err(error_response(request, request_id, err_msg, "invalid_request_error", Some("model_not_found")));
        }

        // Load settings for this request: the user's settings + the request's "llama" params on top
        let mut settings = cluaiz_core::Settings::current();
        if let Some(params) = &request.llama {
            if let Err(e) = params.apply_to(&mut settings.optimization) {
                return Err(error_response(request, request_id, format!("Invalid \"llama\" params: {}", e), "invalid_request_error", Some("invalid_llama_params")));
            }
        }

        // Load (or reuse) the model now, so every token budget below uses the context llama.cpp really created
        let is_gguf = active_model_path.as_ref()
            .and_then(|p| p.extension()).and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gguf")).unwrap_or(false);
        let load_started = std::time::Instant::now();
        if let (Some(model_path), true) = (active_model_path.as_ref(), is_gguf) {
            if let Err(e) = state.host.ensure_loaded(&resolved_model_name, model_path, &settings).await {
                let err_msg = format!("Model could not be loaded: {}", e);
                tracing::error!("❌ [API] {}", err_msg);
                return Err(error_response(request, request_id, err_msg, "engine_load_error", None));
            }
        }
        let model_load_ms = load_started.elapsed().as_millis();
        let start_time = std::time::Instant::now();

        // 🛡️ Dynamic Context Limit: Resolved from InstalledStateRegistry or active slot (Min 2k Floor)
        let installed_registry = cluaiz_models::registry::InstalledStateRegistry::load();
        let active_model_entry = installed_registry
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
            });

        // Context of the loaded model as created by llama.cpp (set by EngineHost after load)
        // GGUF: context llama.cpp created at load. Other formats: the registry's context window.
        let dynamic_context_limit = match (is_gguf, cluaiz_core::active_context_tokens()) {
            (true, Some(n)) => n as usize,
            (true, None) => {
                return Err(axum::response::Json(json!({
                    "error": { "message": "No model context is active after load", "type": "engine_load_error" }
                })).into_response());
            }
            // Unchanged ONNX behaviour (registry value, 2048 floor) — not part of the GGUF planner
            (false, _) => active_model_entry
                .map(|m| cluaiz_models::StructuralDNA::parse_context_string(&m.metadata.context_window))
                .unwrap_or(2048)
                .max(2048),
        };

        // 🧬 Resolve Model Chat Template
        let chat_tmpl = active_model_entry
            .and_then(|entry| entry.metadata.chat_template.clone())
            .or_else(|| {
                active_model_path.as_ref().and_then(|path: &PathBuf| {
                    if path.extension().and_then(|e: &std::ffi::OsStr| e.to_str()) == Some("gguf") {
                        cluaiz_models::GgufProber::probe_file(path).ok().and_then(|probe| {
                            probe.chat_template
                        })
                    } else {
                        None
                    }
                })
            });

        // 🧬 Resolve Dynamic Model Thinking Markers Directly from llama.cpp Native Engine (Zero Hardcoding)
        let (dyn_think_start, dyn_think_end): (Option<String>, Option<String>) = {
            let native_tags = state.host.get_thinking_tags(chat_tmpl.as_deref()).await;
            if native_tags.0.is_some() || native_tags.1.is_some() {
                native_tags
            } else if let Some(entry) = active_model_entry {
                let st = entry.metadata.think_start_tag.clone().filter(|s| !s.is_empty() && !s.contains("tool") && !s.contains("important"));
                let et = entry.metadata.think_end_tag.clone().filter(|s| !s.is_empty() && !s.contains("tool") && !s.contains("important"));
                (st, et)
            } else {
                (None, None)
            }
        };

        let validated_max_tokens = request.max_tokens.map(|t| t.min(dynamic_context_limit));
        let skip_brain = matches!(request.temporary_chat, Some(TemporaryChatMode::Strict));

        let mut augmented_messages = request.messages.clone();
 
        // 🚀 RESOLVE ACTIVE SESSION TOOLS & COMPILE MARKET-STANDARD PROMPT SCHEMAS
        let mut tool_ids_to_compile: Vec<String> = Vec::new();

        if let Some(ref sid) = request.session_id {
            let session_tools = cluaiz_agents::SessionToolManager::get_session_tools(sid);
            for tool_binding in session_tools {
                if !tool_ids_to_compile.contains(&tool_binding.id) {
                    tool_ids_to_compile.push(tool_binding.id);
                }
            }
        }

        if let Some(ref tools_vec) = request.tools {
            for tool_val in tools_vec {
                let tid_opt = tool_val.as_str()
                    .or_else(|| tool_val.get("id").and_then(|v| v.as_str()))
                    .or_else(|| tool_val.get("name").and_then(|v| v.as_str()));
                if let Some(tool_id) = tid_opt {
                    let tid = tool_id.to_string();
                    if !tool_ids_to_compile.contains(&tid) {
                        tool_ids_to_compile.push(tid.clone());
                    }
                    if let Some(turns_val) = tool_val.get("turns").and_then(|v| v.as_i64()) {
                        if let Some(ref sid) = request.session_id {
                            let binding = cluaiz_agents::SessionToolBinding {
                                id: tid,
                                turns: turns_val as i32,
                            };
                            cluaiz_agents::SessionToolManager::update_session_tools(sid, vec![binding], vec![]);
                        }
                    }
                }
            }
        }

        let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        let initial_user_query = last_message.flatten_to_string().await;
        let prompt_lower = initial_user_query.to_lowercase();
        let matched_skills = cluaiz_agents::ToolsEngine::match_skills(&prompt_lower);
        for skill_id in matched_skills {
            if !tool_ids_to_compile.contains(&skill_id) {
                tool_ids_to_compile.push(skill_id);
                break;
            }
        }

        let compiled_tools = cluaiz_agents::ToolPromptCompiler::compile_tools(&tool_ids_to_compile).await;

        let n_ctx_limit = dynamic_context_limit;
        let max_tool_chars = (n_ctx_limit * 4 * 35) / 100;

        let mut prompt_tool_sections = Vec::new();

        if !compiled_tools.tools_xml.is_empty() {
            let tool_instructions = format!(
                "# Available Tools\nYou have access to the following tools:\n{}\n\nWhen you need to call a tool, you MUST output a <tool_call> XML block with a JSON object containing \"name\" and \"arguments\":\n<tool_call>\n{{\"name\": \"tool_name\", \"arguments\": {{\"arg_name\": \"value\"}}}}\n</tool_call>\nDo not output conversational filler text before or after the <tool_call> block.",
                compiled_tools.tools_xml
            );
            prompt_tool_sections.push(tool_instructions);
        }

        if !compiled_tools.skill_instructions.is_empty() {
            prompt_tool_sections.push(compiled_tools.skill_instructions);
        }

        let mut combined_instructions = prompt_tool_sections.join("\n\n---\n\n");
        if combined_instructions.len() > max_tool_chars {
            tracing::warn!("⚠️ [ChatHandler] Tool prompt truncated to prevent Context Window overflow (budget limit: {} chars)", max_tool_chars);
            combined_instructions.truncate(max_tool_chars);
        }

        let _tool_instruction_chars = combined_instructions.len();

        if !combined_instructions.is_empty() {
            if let Some(first_msg) = augmented_messages.first_mut() {
                if first_msg.role.eq_ignore_ascii_case("system") {
                    let prev_content = first_msg.content.flatten_to_string().await;
                    first_msg.content = MessageContent::Text(format!("{}\n\n{}", prev_content, combined_instructions));
                } else {
                    augmented_messages.insert(
                        0,
                        super::types::ExternalMessage {
                            role: "system".to_string(),
                            content: MessageContent::Text(combined_instructions),
                        },
                    );
                }
            } else {
                augmented_messages.push(super::types::ExternalMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text(combined_instructions),
                });
            }
        }

        // 🧠 1. THINKING MONOLOGUE VISIBILITY (think_mode: "on" | "off" | "auto")
        let active_think_mode = request.think_mode.as_ref().map(|v| {
            if let Some(s) = v.as_str() {
                match s.to_lowercase().as_str() {
                    "off" | "false" | "0" | "hide" => "off".to_string(),
                    "on" | "true" | "1" | "show" => "on".to_string(),
                    _ => "auto".to_string(),
                }
            } else if let Some(b) = v.as_bool() {
                if b { "on".to_string() } else { "off".to_string() }
            } else if let Some(n) = v.as_i64() {
                if n == 0 { "off".to_string() } else { "on".to_string() }
            } else {
                "auto".to_string()
            }
        }).unwrap_or_else(|| "auto".to_string());

        // 🧠 2. REASONING EFFORT / COMPUTE INTENSITY (reasoning_effort: "low" | "medium" | "high" | "max" | "auto")
        let active_reasoning_effort = request.reasoning_effort.as_deref()
            .map(|re| match re.to_lowercase().as_str() {
                "low" | "minimal" => "low".to_string(),
                "medium" | "standard" => "medium".to_string(),
                "high" => "high".to_string(),
                "max" | "extreme" => "max".to_string(),
                custom_num => {
                    if custom_num.parse::<usize>().is_ok() {
                        custom_num.to_string()
                    } else {
                        "auto".to_string()
                    }
                }
            })
            .unwrap_or_else(|| "auto".to_string());

        // 🧠 Dynamic Pre-Flight Context Shifting
        let opt_control = &settings.optimization;
        let gen_reserve = validated_max_tokens.unwrap_or(1024).clamp(256, (n_ctx_limit / 4).max(512));
        
        let mut msg_lengths = Vec::new();
        for m in augmented_messages.iter() {
            let content_str = m.content.flatten_to_string().await;
            let est_tokens = (content_str.len() / 4).max(1);
            msg_lengths.push(est_tokens);
        }

        if augmented_messages.len() > 2 {
            let prompt_token_budget = n_ctx_limit.saturating_sub(gen_reserve).max(512);
            let total_est: usize = msg_lengths.iter().sum();
            if total_est > prompt_token_budget && opt_control.context_shifting != cluaiz_core::ContextShiftingMode::Off {
                let tokens_to_drop = total_est.saturating_sub(prompt_token_budget);
                let target_drop = match opt_control.context_shifting {
                    cluaiz_core::ContextShiftingMode::Minimal => ((n_ctx_limit as f32) * 0.05) as usize,
                    cluaiz_core::ContextShiftingMode::Standard => ((n_ctx_limit as f32) * 0.10) as usize,
                    cluaiz_core::ContextShiftingMode::Aggressive => ((n_ctx_limit as f32) * 0.25) as usize,
                    cluaiz_core::ContextShiftingMode::Extreme => ((n_ctx_limit as f32) * 0.50) as usize,
                    _ => tokens_to_drop,
                }.max(tokens_to_drop);

                let has_system = augmented_messages.first().map(|m| m.role.eq_ignore_ascii_case("system")).unwrap_or(false);
                let keep_start = if has_system { 1 } else { 0 };
                let mut dropped = 0;
                while augmented_messages.len() > (keep_start + 1) && dropped < target_drop {
                    let turn_tokens = msg_lengths.get(keep_start).copied().unwrap_or(1);
                    augmented_messages.remove(keep_start);
                    if keep_start < msg_lengths.len() {
                        msg_lengths.remove(keep_start);
                    }
                    dropped += turn_tokens;
                }
                tracing::info!(
                    "🌊 [ContextShifting] Pre-flight sliding window pruned historical turns (~{} tokens). Remaining prompt fits safely within {} budget.",
                    dropped, prompt_token_budget
                );
            }
        }

        // 🎯 Dynamic Per-Request Token Budgeting (z-plan / resource_negotiator rule):
        // free = n_ctx - prompt_tokens; answer_budget = min(max_tokens, free)
        let prompt_tokens_est: usize = msg_lengths.iter().sum();
        let free_tokens = n_ctx_limit.saturating_sub(prompt_tokens_est).max(32);
        let dynamic_answer_budget = match request.max_tokens {
            Some(req_tokens) => req_tokens.min(dynamic_context_limit).min(free_tokens),
            None => free_tokens,
        };
        let validated_max_tokens = Some(dynamic_answer_budget);

        // 🚀 In-Memory Payload & Sampler Dispatch
        let mut serialized_messages = Vec::new();
        let mut system_prompt_chars = 0;
        let mut history_chars = 0;
        let mut user_prompt_chars = 0;
        let total_msgs = augmented_messages.len();

        for (i, msg) in augmented_messages.iter().enumerate() {
            let content_str = msg.content.flatten_to_string().await;
            let c_len = content_str.len();
            if msg.role == "system" {
                system_prompt_chars += c_len;
            } else if i == total_msgs.saturating_sub(1) {
                user_prompt_chars = c_len;
            } else {
                history_chars += c_len;
            }
            serialized_messages.push(json!({
                "role": msg.role,
                "content": content_str
            }));
        }

        // Only what the request sent; missing values stay null so the engine's fallback order applies
        let samplers = json!({
            "temp": request.temperature,
            "top_p": request.top_p,
            "top_k": request.top_k,
            "min_p": request.min_p,
            "presence_penalty": request.presence_penalty,
            "frequency_penalty": request.frequency_penalty,
            "repeat_penalty": request.repetition_penalty,
            "seed": request.seed
        });

        let payload_envelope = json!({
            "messages": serialized_messages,
            "samplers": samplers,
            "think_mode": &active_think_mode,
            "reasoning_effort": &active_reasoning_effort,
            "chat_template_kwargs": request.chat_template_kwargs.as_ref(),
            "grammar": request.grammar.as_deref(),
            "response_format": request.response_format.as_ref()
        });

        let json_prompt = serde_json::to_string(&payload_envelope).unwrap_or_else(|_| "{}".to_string());

        let prompt_starts_in_think = if active_think_mode == "off" {
            false
        } else if let (Some(ref st), Some(ref et)) = (&dyn_think_start, &dyn_think_end) {
            if st.is_empty() || et.is_empty() {
                false
            } else if let Some(ref tmpl) = chat_tmpl {
                if let Some(idx) = tmpl.rfind("add_generation_prompt") {
                    if tmpl.is_char_boundary(idx) {
                        let gen_part = &tmpl[idx..];
                        let last_st = gen_part.rfind(st);
                        let last_et = gen_part.rfind(et);
                        match (last_st, last_et) {
                            (Some(s_idx), Some(e_idx)) => s_idx > e_idx,
                            (Some(_), None) => true,
                            _ => false,
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        Ok(Self {
            request_id: request_id.to_string(),
            target_slot: target_slot.to_string(),
            resolved_model_name,
            active_model_path,
            dynamic_context_limit,
            n_ctx_limit,
            chat_tmpl,
            dyn_think_start,
            dyn_think_end,
            prompt_starts_in_think,
            active_think_mode,
            active_reasoning_effort,
            validated_max_tokens,
            skip_brain,
            samplers,
            system_prompt_chars,
            history_chars,
            user_prompt_chars,
            active_tool_ids: tool_ids_to_compile,
            json_prompt,
            initial_user_query,
            send_telemetry,
            start_time,
            model_load_ms,
        })
    }
}
