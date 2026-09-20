use axum::response::IntoResponse;
use serde_json::{json, Value};
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
    pub active_response_length: String,
    pub validated_max_tokens: Option<usize>,
    pub skip_brain: bool,
    pub effective_temp: f64,
    pub effective_top_p: f64,
    pub effective_top_k: usize,
    pub effective_min_p: f64,
    pub effective_presence: f64,
    pub effective_frequency: f64,
    pub effective_repeat: f64,
    pub effective_seed: Option<i64>,
    pub system_prompt_chars: usize,
    pub history_chars: usize,
    pub user_prompt_chars: usize,
    pub json_prompt: String,
    pub initial_user_query: String,
    pub send_telemetry: bool,
    pub start_time: std::time::Instant,
}

impl PreparedChatContext {
    pub async fn prepare(
        state: &Arc<AppState>,
        request: &ExternalChatRequest,
        request_id: &str,
    ) -> Result<Self, axum::response::Response> {
        let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        let send_telemetry = schema.stream_telemetry;
        let start_time = std::time::Instant::now();

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
            return Err(err_response.into_response());
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
            let installed = engines::models::InstalledStateRegistry::load();
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
            if request.stream {
                let err_id = request_id.to_string();
                let stream = async_stream::stream! {
                    let err_chunk = json!({
                        "id": err_id,
                        "choices": [{"delta": {"content": format!("Error: {}", err_msg)}}]
                    });
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(err_chunk.to_string()));
                    yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data("[DONE]"));
                };
                return Err(axum::response::sse::Sse::new(stream).into_response());
            } else {
                let err_res = json!({
                    "error": {
                        "message": err_msg,
                        "type": "invalid_request_error",
                        "code": "model_not_found"
                    }
                });
                return Err(axum::response::Json(err_res).into_response());
            }
        }

        // 🛡️ Dynamic Context Limit: Resolved from InstalledStateRegistry or active slot (Min 2k Floor)
        let installed_registry = engines::models::InstalledStateRegistry::load();
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

        let dynamic_context_limit = active_model_entry
            .map(|m| cluaiz_shared::metadata::dna::StructuralDNA::parse_context_string(&m.metadata.context_window))
            .unwrap_or(2048)
            .max(2048);

        // 🧬 Resolve Model Chat Template
        let chat_tmpl = active_model_entry
            .and_then(|entry| entry.metadata.chat_template.clone())
            .or_else(|| {
                active_model_path.as_ref().and_then(|path| {
                    if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                        engines::models::GgufProber::probe(path).ok().and_then(|(meta, _, _)| {
                            meta.get("tokenizer.chat_template").cloned()
                        })
                    } else {
                        None
                    }
                })
            });

        // 🧬 Resolve Dynamic Model Thinking Markers Directly from llama.cpp Native Engine
        let (dyn_think_start, dyn_think_end): (Option<String>, Option<String>) = {
            let native_tags = state.dispatcher.get_thinking_tags(chat_tmpl.as_deref());
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
            let session_tools = engines::tools::SessionToolManager::get_session_tools(sid);
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
                            let binding = engines::tools::SessionToolBinding {
                                id: tid,
                                turns: turns_val as i32,
                            };
                            engines::tools::SessionToolManager::update_session_tools(sid, vec![binding], vec![]);
                        }
                    }
                }
            }
        }

        let last_message = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        let initial_user_query = last_message.flatten_to_string().await;
        let prompt_lower = initial_user_query.to_lowercase();
        let matched_skills = engines::tools::ToolsEngine::match_skills(&prompt_lower);
        for skill_id in matched_skills {
            if !tool_ids_to_compile.contains(&skill_id) {
                tool_ids_to_compile.push(skill_id);
                break;
            }
        }

        let compiled_tools = engines::tools::ToolPromptCompiler::compile_tools(&tool_ids_to_compile).await;

        let gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
        let live_active_ctx = cluaiz_shared::hardware::governor::HardwareGovernor::get_active_allocations()
            .iter()
            .find(|p| p.context_size > 0)
            .map(|p| p.context_size);

        let n_ctx_limit = live_active_ctx.unwrap_or(dynamic_context_limit).max(2048);
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

        if !combined_instructions.is_empty() {
            if let Some(last_msg) = augmented_messages.last_mut() {
                let prev_content = last_msg.content.flatten_to_string().await;
                last_msg.content = MessageContent::Text(format!("{}\n\n{}", combined_instructions, prev_content));
            }
        }

        // 🧠 REASONING & THINKING BUDGET RESOLUTION
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

        let active_think_mode = match active_think_mode_owned.to_lowercase().as_str() {
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

        // 🧠 Dynamic Pre-Flight Context Shifting
        let opt_control = cluaiz_shared::hardware::governor::HardwareGovernor::load_optimization_settings().unwrap_or_default();
        let gen_reserve = validated_max_tokens.unwrap_or(1024).clamp(256, (n_ctx_limit / 4).max(512));
        
        if augmented_messages.len() > 2 {
            let prompt_token_budget = n_ctx_limit.saturating_sub(gen_reserve).max(512);
            let mut msg_lengths = Vec::new();
            for m in augmented_messages.iter() {
                let content_str = m.content.flatten_to_string().await;
                let est_tokens = (content_str.len() / 4).max(1);
                msg_lengths.push(est_tokens);
            }
            let total_est: usize = msg_lengths.iter().sum();
            if total_est > prompt_token_budget && opt_control.context_shifting != cluaiz_shared::hardware::schema::optimization::ContextShiftingMode::Off {
                let tokens_to_drop = total_est.saturating_sub(prompt_token_budget);
                let target_drop = match opt_control.context_shifting {
                    cluaiz_shared::hardware::schema::optimization::ContextShiftingMode::Minimal => ((n_ctx_limit as f32) * 0.05) as usize,
                    cluaiz_shared::hardware::schema::optimization::ContextShiftingMode::Standard => ((n_ctx_limit as f32) * 0.10) as usize,
                    cluaiz_shared::hardware::schema::optimization::ContextShiftingMode::Aggressive => ((n_ctx_limit as f32) * 0.25) as usize,
                    cluaiz_shared::hardware::schema::optimization::ContextShiftingMode::Extreme => ((n_ctx_limit as f32) * 0.50) as usize,
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

        // 🚀 In-Memory Payload & Sampler Dispatch
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

        let prompt_starts_in_think = if active_think_mode == "off" {
            false
        } else if let (Some(ref st), Some(ref et)) = (&dyn_think_start, &dyn_think_end) {
            if st.is_empty() || et.is_empty() {
                false
            } else if let Some(ref tmpl) = chat_tmpl {
                let test_msgs = [("user", "test")];
                if let Ok(rendered) = cluaiz_shared::TemplateManager::render_messages(tmpl, &test_msgs, true) {
                    rendered.contains(st) && !rendered.contains(et)
                } else if let Some(idx) = tmpl.rfind("add_generation_prompt") {
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
            active_response_length,
            validated_max_tokens,
            skip_brain,
            effective_temp,
            effective_top_p,
            effective_top_k,
            effective_min_p,
            effective_presence,
            effective_frequency,
            effective_repeat,
            effective_seed,
            system_prompt_chars,
            history_chars,
            user_prompt_chars,
            json_prompt,
            initial_user_query,
            send_telemetry,
            start_time,
        })
    }
}
