use super::context_plan;
use super::types::{ExternalChatRequest, TemporaryChatMode};
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
    /// Tools bound to the chat (already in the prompt), which `find_tools` does not offer again
    pub loaded_tool_ids: Vec<String>,
    /// Characters `load_tool` results may use; None when the model was not given the tool meta-tools
    pub tool_load_room: Option<usize>,
    /// Chat id and key of the current user message, for recording this turn in the chat's
    /// ledger (None for private chats and requests without a session)
    pub ledger_turn: Option<(String, String)>,
    /// The tool hint put in front of the current user message
    pub tool_hint: String,
    /// What the Engine chose about tools for this request (the tools part of `context_report`)
    pub tools_report: serde_json::Value,
    /// The plan of the prompt (the plan part of `context_report`)
    pub plan_report: serde_json::Value,
    /// Messages the client request carried (the answer is logged after them)
    pub message_count: usize,
    /// The memory meta-tools (recall, open_memory) were offered
    pub memory_tools: bool,
    /// Keeps idle-time jobs away until the request ends
    _foreground: super::background::Foreground,
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
        // A chat request in flight: idle-time jobs stop and wait
        let foreground = super::background::Foreground::begin();
        super::background::ensure_started(state);
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
        let document_images = if has_media {
            let capability_check =
                (|| -> anyhow::Result<bool> {
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
                    Ok(supported.contains(&cluaiz_core::Modality::Image)
                        && package.projector.is_some())
                })();
            match capability_check {
                Ok(document_images) => document_images,
                Err(error) => {
                    return Err(error_response(
                        request,
                        request_id,
                        error.to_string(),
                        "invalid_request_error",
                        Some("unsupported_media"),
                    ))
                }
            }
        } else {
            false
        };
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

        // The chat's ledger: what the Engine added to its earlier turns (tool hints and tool
        // steps), which the client does not send back. Private chats keep none.
        let ledger_chat = request.session_id.clone().filter(|_| request.temporary_chat.is_none());
        let ledger = ledger_chat.as_deref().map(cluaiz_agents::ChatLedger::get);
        if let (Some(chat), Some(saved)) = (&ledger_chat, &ledger) {
            // After a restart the chat's loaded tools come back from disk
            cluaiz_agents::SessionToolManager::restore_if_missing(chat, saved.bindings.clone());
        }

        // 🚀 RESOLVE ACTIVE SESSION TOOLS & COMPILE MARKET-STANDARD PROMPT SCHEMAS
        // Tools bound by the client go into the system prompt; tools the model loaded with
        // load_tool stay in the conversation where they were loaded.
        let mut tool_ids_to_compile: Vec<String> = Vec::new();
        let mut inline_tool_ids: Vec<String> = Vec::new();

        if let Some(ref sid) = request.session_id {
            let session_tools = cluaiz_agents::SessionToolManager::get_session_tools(sid);
            for tool_binding in session_tools {
                if tool_binding.inline {
                    inline_tool_ids.push(tool_binding.id);
                } else if !tool_ids_to_compile.contains(&tool_binding.id) {
                    tool_ids_to_compile.push(tool_binding.id);
                }
            }
        }

        // Earlier turns as the model saw them: each user message gets back the block the Engine
        // put in front of it, followed by that turn's tool steps
        let tool_role = super::tool_turns::template_has_tool_role(chat_tmpl.as_deref());
        let history_mode = context_plan::HistoryMode::parse(request.history_mode.as_deref());
        let policy = context_plan::policy(request.context_policy.as_ref());
        let rendered = context_plan::render_history(&request.messages, ledger.as_ref(), tool_role, history_mode, policy.recent_turns_min);
        let mut augmented_messages = rendered.messages;
        let mut origin = rendered.origin;
        // The chat's event log: what the client sent (a preview changes nothing)
        if let (Some(chat), false) = (&ledger_chat, request.preview) {
            super::ingest_events::ingest_request(chat, &request.messages);
        }
        let current_turn_key = rendered.current_key;
        let chat_has_tool_history = ledger.as_ref().is_some_and(|saved| !saved.turns.is_empty());

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
                                inline: false,
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
        // The user's previous message, for short follow-ups ("phir se", "isko chalao")
        let previous_user_query = request
            .messages
            .iter()
            .rev()
            .skip(1)
            .find(|m| m.role.eq_ignore_ascii_case("user"))
            .map(|m| m.content.text_content());
        let before_tools = std::time::Instant::now();

        // tool_choice "none" (OpenAI): the model gets no tools for this request
        let tools_allowed = request.tool_choice.as_ref().and_then(|choice| choice.as_str()) != Some("none");
        if !tools_allowed {
            tool_ids_to_compile.clear();
        }
        // Tools bound to the chat (loaded earlier, or named in the request) go in whole
        let compiled_tools =
            cluaiz_agents::ToolPromptCompiler::compile_tools(&tool_ids_to_compile).await;

        // Room for tools: what the context has left after the answer's reserve and the
        // conversation, but never less than a quarter of it (the engine drops the oldest turns
        // before it drops tools the model needs)
        let n_ctx_limit = dynamic_context_limit;
        let answer_reserve = cluaiz_core::answer_reserve(request.max_tokens.unwrap_or(n_ctx_limit), n_ctx_limit);
        let thinks = dyn_think_start.as_ref().is_some_and(|tag| !tag.is_empty());
        let reserve_tokens = if thinks { answer_reserve * 2 } else { answer_reserve };
        // Token counts from the loaded model's tokenizer (characters ÷ 3.5 when none is loaded)
        let count_tokens = |texts: &[&str]| context_plan::count(&state.host, texts);
        let conversation_texts: Vec<String> = augmented_messages.iter().map(|m| m.content.text_content()).collect();
        let count_started = std::time::Instant::now();
        let (conversation_counts, counted) =
            count_tokens(&conversation_texts.iter().map(String::as_str).collect::<Vec<_>>());
        let conversation_tokens: usize = conversation_counts.iter().sum();
        if counted {
            tracing::info!(
                "🧮 [Tokens] Conversation {} tokens ({} messages) counted in {:.2} ms",
                conversation_tokens,
                conversation_texts.len(),
                count_started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let tool_tokens = n_ctx_limit
            .saturating_sub(reserve_tokens + conversation_tokens)
            .max(n_ctx_limit / 4)
            .min(n_ctx_limit.saturating_sub(reserve_tokens));
        let max_tool_chars = (tool_tokens as f64 * cluaiz_agents::tool_search::CHARS_PER_TOKEN) as usize;

        const TOOLS_HEADER: &str = "# Available Tools\nYou have access to the following tools:\n";
        const TOOLS_CALL_FORMAT: &str = "\n\nWhen you need to call a tool, you MUST output a <tool_call> XML block with a JSON object containing \"name\" and \"arguments\":\n<tool_call>\n{\"name\": \"tool_name\", \"arguments\": {\"arg_name\": \"value\"}}\n</tool_call>\nDo not output conversational filler text before or after the <tool_call> block.";
        // Tools run in the open workspace; name it so the model uses paths relative to it.
        let workspace_note = format!(
            "\nTools run in the project folder {}. Use paths relative to it (\".\" is the project folder).",
            cluaiz_agents::ToolsEngine::workspace_dir().display()
        );

        // Suggestions (cards and titles) for tools that are installed but not loaded. The model
        // loads one with load_tool when it needs it; nothing goes in whole on its own.
        let discovery = tools_allowed && request.tool_search != Some(false);
        // recall / open_memory: offered when the prompt leaves earlier messages of the chat out
        let memory_tools = tools_allowed
            && ledger_chat.is_some()
            && history_mode != context_plan::HistoryMode::Recent
            && request.messages.len() > 1;
        // An open task (work that changed something over several turns, or one the model keeps):
        // looked up only when this chat's tools changed something
        let changed_something = ledger.as_ref().is_some_and(|saved| {
            saved.turns.iter().flat_map(|t| &t.steps).any(|step| {
                let name = step.name.to_lowercase();
                ["write", "edit", "create", "move", "delete", "execute", "run"].iter().any(|w| name.contains(w))
            })
        });
        let open_task = if tools_allowed && changed_something || memory_tools {
            ledger_chat.as_deref().and_then(super::recall::open_task)
        } else {
            None
        };
        let mut hint_lines = String::new();
        let mut offer_meta_tools = false;
        let mut suggested = serde_json::Value::Array(Vec::new());
        let mut search_offered = false;
        if discovery {
            let mut exclude = tool_ids_to_compile.clone();
            exclude.extend(inline_tool_ids.iter().cloned());
            if let Some(ref sid) = request.session_id {
                exclude.extend(cluaiz_agents::SessionToolManager::suppressed_tool_ids(sid));
            }
            let catalog = cluaiz_agents::ToolSearch::catalog(&exclude);
            let candidates = cluaiz_agents::ToolSearch::suggest(&catalog, &initial_user_query, previous_user_query.as_deref());
            if !candidates.is_empty() {
                tracing::info!(
                    "🧰 [Tools] Suggested: {}",
                    candidates
                        .iter()
                        .map(|c| format!("{} {:?} {:?}", c.id, c.level, c.matched))
                        .collect::<Vec<_>>()
                        .join("; ")
                );
                hint_lines = cluaiz_agents::ToolSearch::hint_lines(&catalog, &candidates);
                suggested = cluaiz_agents::ToolSearch::candidates_json(&catalog, &candidates);
            } else if tool_ids_to_compile.is_empty()
                && !catalog.is_empty()
                && cluaiz_agents::tool_search::is_actionable(&initial_user_query)
            {
                hint_lines = "Installed tools are not loaded. If one would help, search them with find_tools.\n".to_string();
                search_offered = true;
            }
            offer_meta_tools = !hint_lines.is_empty()
                || !tool_ids_to_compile.is_empty()
                || !inline_tool_ids.is_empty()
                || chat_has_tool_history;
        }
        // Earlier messages not shown (adaptive / none): the model can look them up, also when
        // tool suggestions are off
        let tool_meta = offer_meta_tools;
        if memory_tools {
            if hint_lines.is_empty() {
                hint_lines = "Earlier messages of this chat are not all shown. To look something up, call recall with a query; open_memory gives one message in full.\n".to_string();
            }
            offer_meta_tools = true;
        }
        let mut meta_schemas = if tool_meta { cluaiz_agents::ToolSearch::meta_schemas() } else { Vec::new() };
        if memory_tools {
            meta_schemas.extend(super::recall::memory_schemas());
        }
        // task_update while a task is open
        if open_task.is_some() && ledger_chat.is_some() && tools_allowed {
            meta_schemas.push(super::recall::task_schema());
            offer_meta_tools = true;
            if hint_lines.is_empty() {
                hint_lines = "To keep track of the current task, call task_update with its goal, stage and checkpoint.\n".to_string();
            }
        }

        // Bound tools: whole schemas and whole skill instructions, or left out; never cut
        let mut system_tools = String::new();
        if !compiled_tools.tools_xml.is_empty() || !compiled_tools.skill_instructions.is_empty() {
            let meta_chars: usize = meta_schemas.iter().map(|m| m.to_string().len() + 1).sum();
            let tools_budget = max_tool_chars.saturating_sub(
                TOOLS_HEADER.len() + TOOLS_CALL_FORMAT.len() + workspace_note.len() + meta_chars,
            );
            let (mut kept_schemas, dropped_tools) =
                compiled_tools.schemas_within(tools_budget, &initial_user_query);
            if !dropped_tools.is_empty() {
                tracing::warn!(
                    "⚠️ [ChatHandler] Tool list exceeds the prompt budget ({} chars); left out: {:?}",
                    tools_budget, dropped_tools
                );
            }
            kept_schemas.extend(meta_schemas.iter().cloned());
            let tools_xml = cluaiz_agents::prompt_compiler::tools_block(&kept_schemas);
            if !tools_xml.is_empty() {
                system_tools = format!("{TOOLS_HEADER}{tools_xml}{TOOLS_CALL_FORMAT}{workspace_note}");
            }
            let instructions = &compiled_tools.skill_instructions;
            if !instructions.is_empty() {
                if system_tools.len() + instructions.len() + 7 <= max_tool_chars {
                    system_tools = [system_tools, instructions.clone()]
                        .into_iter()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");
                } else {
                    tracing::warn!(
                        "⚠️ [ChatHandler] Skill instructions ({} chars) do not fit the {} chars left for tools; left out whole",
                        instructions.len(),
                        max_tool_chars.saturating_sub(system_tools.len())
                    );
                }
            }
        }

        // The hint goes into the current user message (not the system prompt), so the system
        // prompt and the earlier turns stay the same for the KV cache
        let mut hint = String::new();
        if !hint_lines.is_empty() {
            hint.push_str("<tools_hint>\n");
            hint.push_str(&hint_lines);
            if system_tools.is_empty() {
                hint.push_str(&format!(
                    "To call a function, reply with only:\n<tool_call>\n{{\"name\": \"function_name\", \"arguments\": {{\"arg\": \"value\"}}}}\n</tool_call>\nFunctions you can call now:\n{}\n{}",
                    cluaiz_agents::prompt_compiler::tools_block(&meta_schemas),
                    workspace_note.trim_start()
                ));
                hint.push('\n');
            }
            hint.push_str("</tools_hint>");
        }
        let tool_load_room = offer_meta_tools.then(|| max_tool_chars.saturating_sub(system_tools.len() + hint.len()));
        let tokens = cluaiz_agents::tool_search::tokens;
        let (part_tokens, _) = count_tokens(&[&hint, &system_tools]);
        let tools_report = json!({
            "suggested": suggested,
            "search_offered": search_offered,
            "loaded_in_chat": inline_tool_ids.clone(),
            "bound": tool_ids_to_compile.clone(),
            "meta_tools": offer_meta_tools,
            "hint_tokens": part_tokens[0],
            "system_tools_tokens": part_tokens[1],
            "conversation_tokens": conversation_tokens,
            "load_room_tokens": tool_load_room.map(tokens),
            "token_counts": if counted { "tokenizer" } else { "estimated from characters" },
            "estimate": "load costs and load room are estimated from characters"
        });
        let after_tools = std::time::Instant::now();

        context_plan::inject(&mut augmented_messages, &mut origin, &hint, &system_tools);

        // The plan of the final prompt. When the history uses most of the room, the chat's
        // ledger gives back old tool hints and old tool results (batched, at least
        // clear_at_least tokens) and the prompt is rendered again; the engine still drops the
        // oldest turns when the history does not fit after that.
        let mut plan = context_plan::plan_messages(&state.host, &augmented_messages, n_ctx_limit, answer_reserve, thinks, &policy);
        let mut compaction = None;
        if let (Some(chat), Some(saved), Ok(planned)) = (&ledger_chat, &ledger, &plan) {
            if planned.needs_compaction {
                let clear_at_least = (n_ctx_limit / 10).max(512);
                // Down to compact_to_pct of the room, and at least what does not fit
                let over: usize = planned.dropped.iter().map(|d| d.tokens).sum::<usize>().max(
                    planned.used.saturating_sub(planned.available * policy.compact_to_pct as usize / 100),
                );
                let count_one = |text: &str| context_plan::count(&state.host, &[text]).0[0];
                if let Some((compacted, report)) =
                    context_plan::compact(saved, &rendered.earlier_keys, &count_one, clear_at_least, over, &policy)
                {
                    tracing::info!(
                        "🗜️ [Context] Compacted chat {}: {} tokens freed ({} tool hints, {} tool results)",
                        chat, report.freed_tokens, report.blocks_removed, report.results_cleared
                    );
                    // A preview reports the compaction without making it
                    if !request.preview {
                        cluaiz_agents::ChatLedger::put(chat, compacted.clone());
                        if !report.unloaded_tools.is_empty() {
                            cluaiz_agents::SessionToolManager::update_session_tools(chat, Vec::new(), report.unloaded_tools.clone());
                        }
                    }
                    let again =
                        context_plan::render_history(&request.messages, Some(&compacted), tool_role, history_mode, policy.recent_turns_min);
                    augmented_messages = again.messages;
                    origin = again.origin;
                    context_plan::inject(&mut augmented_messages, &mut origin, &hint, &system_tools);
                    plan = context_plan::plan_messages(&state.host, &augmented_messages, n_ctx_limit, answer_reserve, thinks, &policy);
                    compaction = Some(report);
                }
            }
        }
        // Auto-recall: earlier messages the prompt does not show, when they fit this message. The
        // record goes in front of the current message and is kept with the turn (like the hint).
        let mut turn_block = hint.clone();
        let mut recall_report = serde_json::Value::Null;
        if let Some(chat) = &ledger_chat {
            let hidden = context_plan::hidden_messages(request.messages.len(), &origin, &plan);
            if !hidden.is_empty() {
                let cap = plan.as_ref().map_or(0, |p| p.available * policy.evidence_cap_pct as usize / 100);
                if let Some(evidence) = super::recall::auto_recall(chat, &initial_user_query, &hidden, cap, history_mode == context_plan::HistoryMode::None) {
                    if let Some(last) = augmented_messages.last_mut() {
                        context_plan::prepend_text(&mut last.content, &evidence.block);
                    }
                    turn_block = if hint.is_empty() { evidence.block } else { format!("{}

{}", evidence.block, hint) };
                    plan = context_plan::plan_messages(&state.host, &augmented_messages, n_ctx_limit, answer_reserve, thinks, &policy);
                    recall_report = evidence.report;
                } else {
                    recall_report = json!({ "hidden_messages": hidden.len(), "recalled": [] });
                }
            }
        }
        // The open task's checkpoint, when the prompt does not show the whole chat (or was
        // just compacted): the model keeps its place
        let mut task_report = serde_json::Value::Null;
        if let Some(task) = &open_task {
            let hidden = context_plan::hidden_messages(request.messages.len(), &origin, &plan);
            if !hidden.is_empty() || compaction.is_some() {
                let block = super::recall::task_block(task);
                if let Some(last) = augmented_messages.last_mut() {
                    context_plan::prepend_text(&mut last.content, &block);
                }
                turn_block = if turn_block.is_empty() { block } else { format!("{block}\n\n{turn_block}") };
                plan = context_plan::plan_messages(&state.host, &augmented_messages, n_ctx_limit, answer_reserve, thinks, &policy);
                task_report = json!({ "id": task.id, "stage": task.stage, "source": task.source, "shown": true });
            } else {
                task_report = json!({ "id": task.id, "stage": task.stage, "source": task.source, "shown": false });
            }
        }
        let mut plan_report = context_plan::report(&plan, history_mode, &policy, compaction.as_ref());
        plan_report["recall"] = recall_report;
        plan_report["task"] = task_report;

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
                        document_images,
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
        let mut json_prompt = envelope.to_prompt();
        // The KV matching mode travels with the envelope; unknown values are ignored
        if let Some(mode) = request.kv_reuse.as_deref() {
            if matches!(mode, "prefix" | "chunk") {
                if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&json_prompt) {
                    value["kv_reuse"] = json!(mode);
                    json_prompt = value.to_string();
                }
            } else {
                tracing::warn!("⚠️ [ChatHandler] Unknown kv_reuse '{}' ignored (prefix or chunk)", mode);
            }
        }

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
            // Tools in the system prompt; tools loaded in the conversation can still be found
            // again (the turn holding them may have been dropped for room)
            loaded_tool_ids: tool_ids_to_compile.clone(),
            ledger_turn: ledger_chat.clone().zip(current_turn_key),
            message_count: request.messages.len(),
            memory_tools,
            _foreground: foreground,
            tool_hint: turn_block,
            tools_report,
            plan_report,
            active_tool_ids: tool_ids_to_compile,
            tool_load_room,
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
                // Saved once when the turn ends (`TurnMemory`), not after every tool step
                false,
            )
            .await
        }
    }
}

/// Saves the chat's KV once its turn has ended: on `finish` at the normal end, or when dropped
/// (the client went away, or the answer ended early). Tool steps in between are not saved.
pub struct TurnMemory {
    state: Arc<crate::state::AppState>,
    chat: Option<String>,
}

impl TurnMemory {
    pub fn new(state: Arc<crate::state::AppState>, ctx: &PreparedChatContext) -> Self {
        Self { state, chat: ctx.chat_id.clone() }
    }

    /// Saves now, before anything that may unload the model (`keep_alive: 0`)
    pub async fn finish(mut self) {
        if let Some(chat) = self.chat.take() {
            self.state.host.save_chat_memory(&chat).await;
        }
    }
}

impl Drop for TurnMemory {
    fn drop(&mut self) {
        if let Some(chat) = self.chat.take() {
            let state = Arc::clone(&self.state);
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move { state.host.save_chat_memory(&chat).await });
            }
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

/// Puts `block` in front of a message's text (as a first text part for multi-part content)
/// The `context_report` of a request: the tools chosen at the start and the tool meta-tool
/// calls made while answering
pub fn context_report(ctx: &PreparedChatContext, tool_events: &[serde_json::Value]) -> serde_json::Value {
    let mut tools = ctx.tools_report.clone();
    tools["events"] = serde_json::Value::Array(tool_events.to_vec());
    let report = json!({ "tools": tools, "plan": ctx.plan_report });
    // Kept with the chat for GET /v1/chat/{id}/context/report/{request_id}
    if let Some(chat) = &ctx.chat_id {
        context_plan::remember(chat, &ctx.request_id, &report);
    }
    report
}

/// A tool meta-tool call for the report
pub fn tool_event(name: &str, payload: &str, result: &str) -> serde_json::Value {
    let args: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    json!({
        "op": name,
        "target": args.get("id").or_else(|| args.get("query")).cloned(),
        "result_tokens": cluaiz_agents::tool_search::tokens(result.len()),
        "ok": !result.starts_with("No ") && !result.contains("does not fit") && !result.contains("needs about"),
    })
}

/// Records what the Engine added to the turn that just ended (its tool hint and tool steps)
/// and the chat's current tool bindings, so the next request renders this turn the same way
pub fn record_turn(
    state: &Arc<AppState>,
    ctx: &PreparedChatContext,
    steps: Vec<cluaiz_agents::RecordedStep>,
    answer: &str,
    tool_events: &[serde_json::Value],
) {
    let Some((chat, key)) = &ctx.ledger_turn else {
        return;
    };
    super::ingest_events::ingest_turn(chat, &steps, tool_events, ctx.plan_report.get("compaction"), answer, ctx.message_count);
    // Idle-time card for this turn, written by the model while its KV is still hot
    if let (Some(model_path), false) = (&ctx.active_model_path, answer.trim().is_empty()) {
        if let Ok(mut envelope) = serde_json::from_str::<serde_json::Value>(&ctx.json_prompt) {
            if let Some(messages) = envelope.get_mut("messages").and_then(serde_json::Value::as_array_mut) {
                messages.push(json!({ "role": "assistant", "content": answer }));
                super::background::enqueue(
                    state,
                    super::background::EnrichJob {
                        chat: chat.clone(),
                        envelope,
                        user_pos: ctx.message_count.saturating_sub(1),
                        answer_pos: ctx.message_count,
                        model_path: Some(model_path.clone()),
                        settings: Some(ctx.request_settings.clone()),
                        cold: false,
                        episode: None,
                    },
                );
            }
        }
    }
    let mut ledger = cluaiz_agents::ChatLedger::get(chat);
    ledger.set_turn(cluaiz_agents::TurnRecord { key: key.clone(), block: ctx.tool_hint.clone(), steps });
    ledger.bindings = cluaiz_agents::SessionToolManager::get_session_tools(chat);
    cluaiz_agents::ChatLedger::put(chat, ledger);
}
