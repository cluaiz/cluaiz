use super::types::{ExternalChatRequest, MessageContent, TemporaryChatMode};
use crate::state::AppState;
use crate::utils::prepare_authorized_media;
use axum::response::IntoResponse;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

pub struct PreparedChatContext {
    pub request_id: String,
    /// The request's session_id: chat memory on disk is kept per chat
    pub chat_id: Option<String>,
    pub target_slot: String,
    pub resolved_model_name: String,
    pub active_model_path: Option<PathBuf>,
    pub dynamic_context_limit: usize,
    pub n_ctx_limit: usize,
    pub chat_tmpl: Option<String>,
    pub dyn_think_start: Option<String>,
    pub dyn_think_end: Option<String>,
    pub active_think_mode: String,
    pub active_reasoning_effort: cluaiz_core::ThinkingBudget,
    pub reasoning_budget_message: Option<String>,
    pub validated_max_tokens: Option<usize>,
    pub skip_brain: bool,
    /// Request sampler values (llama.cpp names, null = not sent): the engine falls back to
    /// user settings, then model defaults, then llama.cpp defaults
    pub samplers: cluaiz_core::SamplerDefaults,
    pub system_prompt_chars: usize,
    pub history_chars: usize,
    pub user_prompt_chars: usize,
    pub active_tool_ids: Vec<String>,
    pub json_prompt: String,
    pub prepared_media: Option<cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>>,
    media_lease: Option<cluaiz_runtime::media_admission::MediaLease>,
    pub request_settings: cluaiz_core::ActiveUserSettings,
    pub cancel_signal: Arc<std::sync::atomic::AtomicBool>,
    pub progress: Option<cluaiz_core::processing::ProcessingReporter>,
    pub skip_reasoning: Arc<std::sync::atomic::AtomicBool>,
    pub initial_user_query: String,
    pub send_telemetry: bool,
    /// When the model was ready (after any load): TTFT and total time count from here
    pub start_time: std::time::Instant,
    /// Time spent loading the model for this request (0 when it was already loaded)
    pub model_load_ms: u128,
}

/// Error reply in the shape the client asked for (SSE chunk + [DONE] when streaming, JSON otherwise).
pub(super) fn error_response(
    request: &ExternalChatRequest,
    request_id: &str,
    message: String,
    error_type: &str,
    code: Option<&str>,
) -> axum::response::Response {
    let details = if error_type == "permission_error" {
        serde_json::from_str::<serde_json::Value>(&message).ok()
    } else {
        None
    };
    let message = details
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(|value| value.as_str())
        .unwrap_or(&message)
        .to_owned();
    let mut error = json!({ "message": message, "type": error_type, "code": code });
    if let Some(details) = details {
        error["details"] = details;
    }
    if request.stream {
        let err_id = request_id.to_string();
        let stream = async_stream::stream! {
            let err_chunk = json!({
                "id": err_id,
                "error": error
            });
            yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data(err_chunk.to_string()));
            yield Ok::<_, std::convert::Infallible>(axum::response::sse::Event::default().data("[DONE]"));
        };
        return axum::response::sse::Sse::new(stream).into_response();
    }
    let status = if code == Some("media_busy") {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    } else if error_type == "permission_error" {
        axum::http::StatusCode::FORBIDDEN
    } else if error_type == "engine_error" || error_type == "engine_load_error" {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    } else {
        axum::http::StatusCode::BAD_REQUEST
    };
    (
        status,
        axum::response::Json(json!({
            "error": error
        })),
    )
        .into_response()
}

impl PreparedChatContext {
    pub async fn prepare(
        state: &Arc<AppState>,
        request: &ExternalChatRequest,
        request_id: &str,
        headers: &axum::http::HeaderMap,
        progress: Option<cluaiz_core::processing::ProcessingReporter>,
    ) -> Result<Self, axum::response::Response> {
        let preflight_started = std::time::Instant::now();
        let schema = cluaiz_security::PermissionSchema::load();
        let send_telemetry = schema.stream_telemetry;

        // The selected chat model owns its multimodal inputs; an image does not substitute a slot model.
        let target_slot = "chat_slot";
        let has_media = request
            .messages
            .iter()
            .any(|message| message.content.has_media());
        let media_lease = if has_media {
            Some(state.host.media_admission.try_acquire().map_err(|_| {
                error_response(request, request_id,
                    "Another chat attachment request still owns processing buffers; retry after it finishes".into(),
                    "resource_error", Some("media_busy"))
            })?)
        } else {
            None
        };

        crate::utils::slots::log_capability_guidance(
            &schema,
            target_slot,
            &[
                "chat-completion",
                "text-generation",
                "vision-chat",
                "multimodal-vision",
            ],
        );

        let mut active_model_path = crate::utils::slots::resolve_model_path(&schema, target_slot);
        let mut resolved_model_name = "default-system-model".to_string();

        if let Some(ref m_id) = request.model {
            if !m_id.trim().is_empty() {
                if let Some(explicit_path) = crate::utils::slots::resolve_model_by_id(m_id) {
                    tracing::info!(
                        "🤖 [API] Model override requested. Resolved '{}' to {:?}",
                        m_id,
                        explicit_path
                    );
                    active_model_path = Some(explicit_path);
                    resolved_model_name = m_id.clone();
                } else {
                    return Err(error_response(
                        request,
                        request_id,
                        format!("Requested model '{}' was not found", m_id),
                        "invalid_request_error",
                        Some("model_not_found"),
                    ));
                }
            }
        } else {
            tracing::info!(
                "🤖 [API] No model specified (null). Falling back to default '{}' model.",
                target_slot
            );
        }

        // Ensure we actually have a path to load (Auto-heal fallback)
        if active_model_path.is_none() && !has_media {
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
            let err_msg = format!(
                "No model is currently loaded in slot '{}' and no valid override was provided.",
                target_slot
            );
            return Err(error_response(
                request,
                request_id,
                err_msg,
                "invalid_request_error",
                Some("model_not_found"),
            ));
        }

        // Load settings for this request: the user's settings + the request's "llama" params on top
        let mut settings = cluaiz_core::Settings::current();
        if let Some(params) = &request.llama {
            if let Err(e) = params.apply_to(&mut settings.optimization) {
                return Err(error_response(
                    request,
                    request_id,
                    format!("Invalid \"llama\" params: {}", e),
                    "invalid_request_error",
                    Some("invalid_llama_params"),
                ));
            }
        }

        // Load (or reuse) the model now, so every token budget below uses the context llama.cpp really created
        let is_gguf = active_model_path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gguf"))
            .unwrap_or(false);
        if has_media {
            let capability_check = (|| -> anyhow::Result<()> {
                if !is_gguf {
                    anyhow::bail!("Typed ONNX chat media dispatch is not implemented yet");
                }
                let model = active_model_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("No selected model"))?;
                let package = cluaiz_models::capabilities::PackageCapabilities::inspect(model)?;
                let supported = package.supported_inputs();
                let mut requires_projector = false;
                for message in &request.messages {
                    for part in message.content.input_parts()? {
                        let input = match part {
                            cluaiz_core::media::InputPart::Image { .. }
                            | cluaiz_core::media::InputPart::Video { .. } => {
                                cluaiz_core::Modality::Image
                            }
                            cluaiz_core::media::InputPart::Audio { .. } => {
                                cluaiz_core::Modality::Audio
                            }
                            _ => cluaiz_core::Modality::Text,
                        };
                        requires_projector |= input != cluaiz_core::Modality::Text;
                        if matches!(
                            part,
                            cluaiz_core::media::InputPart::Video {
                                include_audio: true,
                                ..
                            }
                        ) && !supported.contains(&cluaiz_core::Modality::Audio)
                        {
                            anyhow::bail!(
                                "Video soundtrack requires selected model audio input support"
                            );
                        }
                        if !supported.contains(&input) {
                            anyhow::bail!(
                                "Selected model does not declare {:?} input support",
                                input
                            );
                        }
                    }
                }
                if requires_projector && package.projector.is_none() {
                    anyhow::bail!("Media projector is missing or ambiguous");
                }
                Ok(())
            })();
            if let Err(error) = capability_check {
                return Err(error_response(
                    request,
                    request_id,
                    error.to_string(),
                    "invalid_request_error",
                    Some("unsupported_media"),
                ));
            }
        }
        let load_started = std::time::Instant::now();
        if let Some(p) = &progress {
            p.emit(
                cluaiz_core::processing::ProcessingStage::ModelLoading,
                cluaiz_core::processing::ProcessingState::Running,
                None,
                None,
            );
        }
        if let (Some(model_path), true) = (active_model_path.as_ref(), is_gguf) {
            if let Err(e) = state
                .host
                .ensure_loaded(&resolved_model_name, model_path, &settings)
                .await
            {
                let err_msg = format!("Model could not be loaded: {}", e);
                tracing::error!("❌ [API] {}", err_msg);
                return Err(error_response(
                    request,
                    request_id,
                    err_msg,
                    "engine_load_error",
                    None,
                ));
            }
        }
        let model_load_ms = load_started.elapsed().as_millis();
        if let Some(p) = &progress {
            p.emit(
                cluaiz_core::processing::ProcessingStage::ModelLoading,
                cluaiz_core::processing::ProcessingState::Completed,
                None,
                None,
            );
        }
        let start_time = std::time::Instant::now();

        // 🛡️ Dynamic Context Limit: Resolved from InstalledStateRegistry or active slot (Min 2k Floor)
        let after_load = std::time::Instant::now();
        let installed_registry = cluaiz_models::registry::InstalledStateRegistry::load();
        let active_model_entry = installed_registry
            .installed_models
            .get(&resolved_model_name)
            .or_else(|| {
                schema
                    .active_slots
                    .get(target_slot)
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
                .map(|m| {
                    cluaiz_models::StructuralDNA::parse_context_string(&m.metadata.context_window)
                })
                .unwrap_or(2048)
                .max(2048),
        };

        // 🧬 Resolve Model Chat Template
        let chat_tmpl = active_model_entry
            .and_then(|entry| entry.metadata.chat_template.clone())
            .or_else(|| {
                active_model_path.as_ref().and_then(|path: &PathBuf| {
                    if path.extension().and_then(|e: &std::ffi::OsStr| e.to_str()) == Some("gguf") {
                        cluaiz_models::GgufProber::probe_file(path)
                            .ok()
                            .and_then(|probe| probe.chat_template)
                    } else {
                        None
                    }
                })
            });

        // 🧬 Resolve Dynamic Model Thinking Markers Directly from llama.cpp Native Engine (Zero Hardcoding)
        let (dyn_think_start, dyn_think_end): (Option<String>, Option<String>) =
            {
                let native_tags = state.host.get_thinking_tags(chat_tmpl.as_deref()).await;
                if native_tags.0.is_some() || native_tags.1.is_some() {
                    native_tags
                } else if let Some(entry) = active_model_entry {
                    let st = entry.metadata.think_start_tag.clone().filter(|s| {
                        !s.is_empty() && !s.contains("tool") && !s.contains("important")
                    });
                    let et = entry.metadata.think_end_tag.clone().filter(|s| {
                        !s.is_empty() && !s.contains("tool") && !s.contains("important")
                    });
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
                let tid_opt = tool_val
                    .as_str()
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
                            cluaiz_agents::SessionToolManager::update_session_tools(
                                sid,
                                vec![binding],
                                vec![],
                            );
                        }
                    }
                }
            }
        }

        let last_message = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let initial_user_query = last_message.text_content();
        let prompt_lower = initial_user_query.to_lowercase();
        let before_tools = std::time::Instant::now();
        let matched_skills = cluaiz_agents::ToolsEngine::match_skills(&prompt_lower);
        for skill_id in matched_skills {
            if !tool_ids_to_compile.contains(&skill_id) {
                tool_ids_to_compile.push(skill_id);
                break;
            }
        }

        let compiled_tools =
            cluaiz_agents::ToolPromptCompiler::compile_tools(&tool_ids_to_compile).await;
        let after_tools = std::time::Instant::now();

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
                    match &mut first_msg.content {
                        MessageContent::Text(text) => {
                            text.push_str("\n\n");
                            text.push_str(&combined_instructions);
                        }
                        MessageContent::Array(parts) => {
                            parts.push(super::types::ContentPart::Text {
                                text: combined_instructions,
                            })
                        }
                    }
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
        let active_think_mode = request
            .think_mode
            .as_ref()
            .map(|v| {
                if let Some(s) = v.as_str() {
                    match s.to_lowercase().as_str() {
                        "off" | "false" | "0" | "hide" => "off".to_string(),
                        "on" | "true" | "1" | "show" => "on".to_string(),
                        _ => "auto".to_string(),
                    }
                } else if let Some(b) = v.as_bool() {
                    if b {
                        "on".to_string()
                    } else {
                        "off".to_string()
                    }
                } else if let Some(n) = v.as_i64() {
                    if n == 0 {
                        "off".to_string()
                    } else {
                        "on".to_string()
                    }
                } else {
                    "auto".to_string()
                }
            })
            .unwrap_or_else(|| settings.optimization.think_mode.name().to_string()); // no think_mode: the user's default

        // 🧠 2. Thinking budget ("Effort"): the request's value, else the user's default setting
        let active_reasoning_effort = request
            .reasoning_effort
            .unwrap_or(settings.optimization.reasoning_effort);
        let reasoning_budget_message = request
            .reasoning_budget_message
            .clone()
            .or_else(|| settings.optimization.reasoning_budget_message.clone());

        let mut chat_messages = Vec::with_capacity(augmented_messages.len());
        for m in &augmented_messages {
            chat_messages.push(cluaiz_core::ChatMessage::new(
                m.role.clone(),
                m.content.text_content(),
            ));
        }

        // 🎯 Answer limit, text and media alike: the request's max_tokens, else until the model ends
        // or the context does (llama.cpp's n_predict -1). Fitting the history into the context
        // (context shifting) happens in the engine with the model's own token counts.
        let validated_max_tokens = Some(validated_max_tokens.unwrap_or(n_ctx_limit));

        let mut system_prompt_chars = 0;
        let mut history_chars = 0;
        let mut user_prompt_chars = 0;
        let total_msgs = chat_messages.len();
        for (i, msg) in chat_messages.iter().enumerate() {
            let c_len = msg.content.len();
            if msg.role == "system" {
                system_prompt_chars += c_len;
            } else if i == total_msgs.saturating_sub(1) {
                user_prompt_chars = c_len;
            } else {
                history_chars += c_len;
            }
        }

        // Only what the request sent; missing values stay null so the engine's fallback order applies
        let samplers = cluaiz_core::SamplerDefaults {
            temp: request.temperature,
            top_p: request.top_p,
            top_k: request.top_k,
            min_p: request.min_p,
            presence_penalty: request.presence_penalty,
            frequency_penalty: request.frequency_penalty,
            repeat_penalty: request.repetition_penalty,
            seed: request.seed.and_then(|s| u32::try_from(s).ok()),
        };

        let envelope = cluaiz_core::ChatEnvelope {
            messages: chat_messages,
            samplers: samplers.clone(),
            think_mode: active_think_mode.clone(),
            reasoning_effort: active_reasoning_effort,
            reasoning_budget_message: reasoning_budget_message.clone(),
            chat_template_kwargs: request.chat_template_kwargs.clone(),
            grammar: request.grammar.clone(),
            response_format: request.response_format.clone(),
        };
        let cancel_signal = progress
            .as_ref()
            .map(|p| p.cancellation())
            .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let prepared_media = if let Some(lease) = media_lease.clone() {
            if let Some(p) = &progress {
                p.set_message_offset(
                    augmented_messages
                        .len()
                        .saturating_sub(request.messages.len()),
                );
            }
            let typed = envelope
                .clone()
                .try_map_messages(|_| async {
                    augmented_messages
                        .iter()
                        .map(|message| {
                            Ok(cluaiz_core::ChatMessage {
                                role: message.role.clone(),
                                content: message.content.input_parts()?,
                            })
                        })
                        .collect::<anyhow::Result<Vec<_>>>()
                })
                .await
                .map_err(|error| {
                    error_response(
                        request,
                        request_id,
                        error.to_string(),
                        "invalid_request_error",
                        Some("invalid_media"),
                    )
                })?;
            let (owned_state, owned_headers, owned_settings) =
                (state.clone(), headers.clone(), settings.clone());
            let (cancellation, observer) = (cancel_signal.clone(), progress.clone());
            // A disconnected preflight must not release the permit while spawn_blocking decodes.
            let (prepared, _) = lease
                .prepare(cancel_signal.clone(), async move {
                    prepare_authorized_media(
                        &owned_state,
                        &owned_headers,
                        typed,
                        &owned_settings,
                        Some(cancellation),
                        observer,
                    )
                    .await
                })
                .await
                .map_err(|error| {
                    error_response(
                        request,
                        request_id,
                        format!("Media preparation task failed: {error}"),
                        "engine_error",
                        None,
                    )
                })?;
            Some(prepared.map_err(|(kind, error)| {
                error_response(
                    request,
                    request_id,
                    error,
                    kind,
                    Some("media_preparation_failed"),
                )
            })?)
        } else {
            None
        };
        let json_prompt = envelope.to_prompt();

        // Where the time before the first token goes (model load is reported on its own)
        tracing::info!(
            "⏱️ [Preflight] {} ms total | model load {} ms | registry + template + tags {} ms | skills + tools {} ms | rest {} ms",
            preflight_started.elapsed().as_millis(),
            model_load_ms,
            before_tools.duration_since(after_load).as_millis(),
            after_tools.duration_since(before_tools).as_millis(),
            after_tools.elapsed().as_millis(),
        );

        Ok(Self {
            request_id: request_id.to_string(),
            // A temporary (private) chat keeps nothing on disk, so it gets no chat memory
            chat_id: request
                .session_id
                .clone()
                .filter(|_| request.temporary_chat.is_none()),
            target_slot: target_slot.to_string(),
            resolved_model_name,
            active_model_path,
            dynamic_context_limit,
            n_ctx_limit,
            chat_tmpl,
            dyn_think_start,
            dyn_think_end,
            active_think_mode,
            active_reasoning_effort,
            reasoning_budget_message,
            validated_max_tokens,
            skip_brain,
            samplers,
            system_prompt_chars,
            history_chars,
            user_prompt_chars,
            active_tool_ids: tool_ids_to_compile,
            json_prompt,
            prepared_media,
            media_lease,
            request_settings: settings,
            cancel_signal,
            progress,
            skip_reasoning: Arc::new(std::sync::atomic::AtomicBool::new(
                request.skip_reasoning == Some(true),
            )),
            initial_user_query,
            send_telemetry,
            start_time,
            model_load_ms,
        })
    }

    /// Both streaming modes and tool continuations use this typed dispatch boundary.
    pub async fn dispatch(
        &self,
        host: &cluaiz_runtime::EngineHost,
        pivot: Option<&str>,
        max_tokens: usize,
    ) -> cluaiz_runtime::HostResponse {
        if let Some(prepared) = &self.prepared_media {
            let prepared = match media_continuation(prepared, pivot) {
                Ok(prepared) => prepared,
                Err(error) => return cluaiz_runtime::HostResponse::Error(error.to_string()),
            };
            let Some(path) = &self.active_model_path else {
                return cluaiz_runtime::HostResponse::Error(
                    "Selected media model identity is missing".into(),
                );
            };
            host.dispatch_prepared_with_progress(
                prepared,
                max_tokens,
                path,
                &self.request_settings,
                Some(Arc::clone(&self.cancel_signal)),
                Some(Arc::clone(&self.skip_reasoning)),
                self.progress.clone(),
                self.media_lease.clone(),
                self.chat_id.as_deref(),
            )
            .await
        } else {
            let Some(path) = &self.active_model_path else {
                return cluaiz_runtime::HostResponse::Error(
                    "Selected model identity is missing".into(),
                );
            };
            host.dispatch_chat_text(
                pivot.unwrap_or(&self.json_prompt),
                max_tokens,
                path,
                &self.request_settings,
                Arc::clone(&self.cancel_signal),
                Arc::clone(&self.skip_reasoning),
                self.chat_id.as_deref(),
            )
            .await
        }
    }
}

/// Regenerate full multimodal context; text-only KV pivot reuse cannot represent media embeddings.
pub fn media_continuation(
    prepared: &cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>,
    pivot: Option<&str>,
) -> anyhow::Result<cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>> {
    use anyhow::Context;
    let mut continued = prepared.clone();
    if let Some(pivot) = pivot {
        let value: serde_json::Value =
            serde_json::from_str(pivot.strip_prefix("[PIVOT_CONTINUE]").unwrap_or(pivot))?;
        let text = value
            .get("pivot_prompt")
            .and_then(|value| value.as_str())
            .context("Missing media tool continuation prompt")?;
        continued
            .messages
            .last_mut()
            .context("Media continuation has no messages")?
            .content
            .push(cluaiz_core::media::PreparedPart::Text {
                text: format!("\n{text}"),
            });
    }
    Ok(continued)
}
