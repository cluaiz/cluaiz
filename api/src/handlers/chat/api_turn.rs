//! The API chat's part of an agent turn (`cluaiz_agents::turn`), for streaming and non-streaming
//! requests alike: generation through the prepared context (the first dispatch, then a prompt
//! continuation with the tool steps), the Engine's tools (meta tools, skills, plugins, MCP) with the
//! approval gate, and the SSE chunks a streaming client sees.

use super::preflight::PreparedChatContext;
use super::tool_parser;
use super::types::{ExternalChatRequest, ACTIVE_STREAMS, DEFAULT_MAX_TOKENS};
use crate::state::AppState;
use chrono::Utc;
use cluaiz_agents::turn::{CallRun, Generated, ToolCall};
use cluaiz_runtime::HostResponse;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Default maximum generations in one API turn (`0` = unlimited, guarded by repeat detection)
pub const MAX_STEPS: usize = 100;

/// Resolves the step limit for an API turn (`request.max_steps`, defaulting to [`MAX_STEPS`])
pub fn max_steps_for(request: &ExternalChatRequest) -> usize {
    request.max_steps.unwrap_or(MAX_STEPS)
}

pub struct ApiTurn<'a> {
    state: Arc<AppState>,
    ctx: &'a PreparedChatContext,
    request: &'a ExternalChatRequest,
    session_id: Option<String>,
    /// The first generation, dispatched by the handler before the turn
    first: Option<HostResponse>,
    /// SSE `data:` payloads for a streaming client; None for a non-streaming request
    sink: Option<UnboundedSender<String>>,
    pub tool_turns: super::tool_turns::ToolTurns,
    pub tool_events: Vec<Value>,
    /// A tool the last call unloaded: its earlier load result becomes a short note
    unloaded: Option<String>,
    /// The engine's error that ended the turn (message, error type)
    pub error: Option<(String, &'static str)>,
    /// The client-run call that ended the turn, as OpenAI `tool_calls`
    pub client_call: Option<Value>,
    /// The raw text of the last generation (non-streaming separates its reasoning)
    pub last_raw: String,
    // Streaming display state
    think_filter: cluaiz_models::StreamingReasoningFilter,
    call_text_guard: tool_parser::ToolCallTextGuard,
    /// The answer as the client sees it (logged in the chat's event log)
    pub visible_answer: String,
    pub token_count: usize,
    pub reasoning_tokens: usize,
    pub first_token_ms: u128,
    /// The project's items the chat has whole (this message's and earlier ones'): a glob rule of
    /// a file a tool touched is added to its result only when it is not one of them
    project_sent: Vec<String>,
    /// A confirmation went unanswered: nobody is there, so this turn asks no more and waits no more
    nobody_answered: bool,
}

impl<'a> ApiTurn<'a> {
    pub fn new(
        state: Arc<AppState>,
        ctx: &'a PreparedChatContext,
        request: &'a ExternalChatRequest,
        first: HostResponse,
        sink: Option<UnboundedSender<String>>,
    ) -> Self {
        let mut tool_events = Vec::new();
        // A tool the Engine loaded before the model answered (auto_load) shows like a load_tool call
        if let Some(step) = &ctx.auto_loaded {
            let payload = json!({ "id": ctx.tools_report["auto_loaded"] }).to_string();
            tool_events.push(super::preflight::tool_event(cluaiz_agents::LOAD_TOOL_NAME, &payload, &step.result));
        }
        let mut project_sent = ctx.ledger_turn.as_ref().map(|(chat, _)| cluaiz_agents::ChatLedger::get(chat).project_sent).unwrap_or_default();
        project_sent.extend(ctx.project_sent.iter().cloned());
        Self {
            project_sent,
            nobody_answered: false,
            state,
            ctx,
            request,
            session_id: request.session_id.clone(),
            first: Some(first),
            sink,
            tool_turns: super::tool_turns::ToolTurns::start(ctx),
            tool_events,
            unloaded: None,
            error: None,
            client_call: None,
            last_raw: String::new(),
            think_filter: cluaiz_models::StreamingReasoningFilter::new(
                ctx.dyn_think_start.clone(),
                ctx.dyn_think_end.clone(),
                // The engine sends the start tag itself when the rendered prompt opened the block
                false,
                ctx.active_think_mode == "off",
            ),
            call_text_guard: tool_parser::ToolCallTextGuard::default(),
            visible_answer: String::new(),
            token_count: 0,
            reasoning_tokens: 0,
            first_token_ms: 0,
        }
    }

    fn streaming(&self) -> bool {
        self.sink.is_some()
    }

    /// Sends one SSE payload (streaming only)
    fn send(&self, data: Value) {
        if let Some(sink) = &self.sink {
            let _ = sink.send(data.to_string());
        }
    }

    fn chunk(&self, model: Option<&str>, choices: Value) -> Value {
        json!({
            "id": self.ctx.request_id.clone(),
            "object": "chat.completion.chunk",
            "created": Utc::now().timestamp(),
            "model": model.map(str::to_owned).unwrap_or_else(|| self.ctx.resolved_model_name.clone()),
            "choices": choices
        })
    }

    /// Whether the client asked to skip the reasoning (chat.skip_reasoning)
    pub fn skip_reasoning(&self) -> bool {
        ACTIVE_STREAMS
            .read()
            .ok()
            .and_then(|lock| lock.get(&self.ctx.request_id).map(|s| s.skip_reasoning.load(Ordering::Relaxed)))
            .unwrap_or(false)
    }

    /// The text the think filter and the call guard still hold, sent as the last delta
    pub fn flush(&mut self) {
        let should_skip = self.skip_reasoning();
        let (final_content, final_reasoning) = self.think_filter.flush_final();
        // Held text that never became a complete tool call is shown as answer text.
        let final_content = match (self.call_text_guard.flush(), final_content) {
            (Some(held), Some(rest)) => Some(held + &rest),
            (held, rest) => held.or(rest),
        };
        if !should_skip && final_reasoning.is_some() {
            self.reasoning_tokens += 1;
        }
        if final_content.is_some() || (!should_skip && final_reasoning.is_some()) {
            let mut delta = serde_json::Map::new();
            if let Some(c) = final_content {
                self.visible_answer.push_str(&c);
                delta.insert("content".to_string(), Value::String(c));
            }
            if let Some(r) = final_reasoning {
                if !should_skip {
                    delta.insert("reasoning_content".to_string(), Value::String(r));
                }
            }
            self.send(self.chunk(None, json!([{ "delta": delta }])));
        }
    }

    /// A generated token: shown as answer or reasoning text (streaming)
    fn show_token(&mut self, token: &str) {
        self.token_count += 1;
        if self.token_count == 1 {
            self.first_token_ms = self.ctx.start_time.elapsed().as_millis();
        }
        if !self.streaming() {
            return;
        }
        let should_skip = self.skip_reasoning();
        let (content, reasoning) = self.think_filter.process_token(token);
        let content = content.and_then(|c| self.call_text_guard.push(&c));
        if !should_skip && reasoning.is_some() {
            self.reasoning_tokens += 1;
        }
        if let Some(text) = &content {
            self.visible_answer.push_str(text);
        }
        let reasoning = if should_skip { None } else { reasoning };
        if content.is_some() || reasoning.is_some() {
            let mut delta = serde_json::Map::new();
            if let Some(c) = content {
                delta.insert("content".to_string(), Value::String(c));
            }
            if let Some(r) = reasoning {
                delta.insert("reasoning_content".to_string(), Value::String(r));
            }
            self.send(self.chunk(None, json!([{ "delta": delta }])));
        }
    }
}

impl cluaiz_agents::turn::Agent for ApiTurn<'_> {
    async fn generate(&mut self, step: usize) -> anyhow::Result<Generated> {
        let response = match self.first.take() {
            Some(first) => first,
            None => {
                // The next step renders the conversation with the calls made so far
                self.think_filter.reset_turn(false);
                let prompt = self.tool_turns.continuation(self.ctx);
                tracing::info!("🔄 [API] Step {step}: generating again with the tool result");
                self.ctx
                    .dispatch(&self.state.host, Some(&prompt), self.ctx.validated_max_tokens.unwrap_or(DEFAULT_MAX_TOKENS))
                    .await
            }
        };
        let mut rx = match response {
            HostResponse::TokenStream(rx) => rx,
            HostResponse::Completed(text) => {
                self.show_token(&text);
                self.last_raw = text.clone();
                return Ok(Generated::Answer { said: text, calls: Vec::new() });
            }
            HostResponse::Error(error) => {
                self.error = Some((error.clone(), error_kind(self.ctx, &error)));
                anyhow::bail!(error);
            }
        };
        let mut text = String::new();
        while let Some(token) = rx.recv().await {
            if self.ctx.cancel_signal.load(Ordering::Relaxed) {
                tracing::info!("🛑 [StreamControl] Stopping '{}' on the user's cancel", self.ctx.request_id);
                self.last_raw = text.clone();
                return Ok(Generated::Stopped { said: text });
            }
            if token.trim() == "[DONE]" {
                break;
            }
            if let Some(error) = cluaiz_runtime::stream_error(&token) {
                self.error = Some((error.to_owned(), error_kind(self.ctx, error)));
                anyhow::bail!(error.to_owned());
            }
            text.push_str(&token);
            if let Some(call) = tool_parser::extract_tool_call(&text) {
                // The call is complete: stop generating now (the worker stops at its next token),
                // instead of after the tool ran or its approval was answered
                rx.close();
                self.call_text_guard.clear();
                self.last_raw = text.clone();
                let said = text.split(cluaiz_core::tool_calls::OPEN).next().unwrap_or_default().to_owned();
                return Ok(Generated::Answer {
                    said,
                    calls: vec![ToolCall { name: call.raw_fn_name, arguments: call.payload_str, raw: call.raw_json }],
                });
            }
            self.show_token(&token);
        }
        self.last_raw = text.clone();
        Ok(Generated::Answer { said: text, calls: Vec::new() })
    }

    async fn call(&mut self, call: &ToolCall) -> CallRun {
        let (ctx, request) = (self.ctx, self.request);
        let session_id = self.session_id.clone();
        let raw_fn_name = call.name.as_str();
        let payload_str = call.arguments.clone();

        // A function the client runs itself: the call goes back as OpenAI `tool_calls`
        if ctx.client_tools.iter().any(|name| name == raw_fn_name) {
            let calls = json!([{
                "id": format!("call_{}", Utc::now().timestamp_millis()),
                "type": "function",
                "function": {"name": raw_fn_name, "arguments": payload_str}
            }]);
            let mut indexed = calls.clone();
            indexed[0]["index"] = json!(0);
            self.send(self.chunk(None, json!([{ "index": 0, "delta": { "tool_calls": indexed } }])));
            self.client_call = Some(calls);
            return CallRun::Client;
        }

        // find_tools / load_tool / unload_tool, memory and project reads are answered by the Engine
        // itself; a tool called by its id before it was loaded is answered as load_tool
        let implicit_load = super::tool_turns::needs_load(ctx, &self.tool_turns, session_id.as_deref(), raw_fn_name);
        let is_meta_tool = implicit_load
            || (ctx.tool_load_room.is_some() && cluaiz_agents::ToolSearch::is_meta_tool(raw_fn_name))
            || ((ctx.memory_tools || raw_fn_name == super::recall::TASK_UPDATE_TOOL)
                && ctx.ledger_turn.is_some()
                && super::recall::is_memory_tool(raw_fn_name))
            || (ctx.project_read && raw_fn_name == cluaiz_agents::workspace::READ_ITEM);
        let (meta_name, meta_payload) = if implicit_load {
            (cluaiz_agents::LOAD_TOOL_NAME, json!({"id": raw_fn_name, "implicit": true}).to_string())
        } else {
            (raw_fn_name, payload_str.clone())
        };
        let resolved_target = if is_meta_tool {
            None
        } else if let Some(ref sid) = session_id {
            cluaiz_agents::ToolsEngine::resolve_function_target_for_session(sid, raw_fn_name)
                .or_else(|| cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[]))
        } else {
            cluaiz_agents::ToolsEngine::resolve_function_target(raw_fn_name, &[])
        };
        let (comp_type, comp_name, tool_sub_func) = if is_meta_tool {
            ("tools".to_string(), raw_fn_name.to_string(), None)
        } else if let Some(target) = resolved_target {
            (target.category, target.component_id, target.sub_function)
        } else {
            ("plugin".to_string(), raw_fn_name.to_string(), None)
        };
        let public_call_name = comp_name.clone();
        tracing::info!("🔍 [API] Tool call: {} '{}' (sub_func: {:?}) with payload: {}", comp_type, public_call_name, tool_sub_func, payload_str);

        let icon_svg = if self.streaming() { tool_icon(&comp_type, &comp_name) } else { None };
        let model = request.model.as_deref();
        // The OpenAI tool_calls delta, before the call runs
        self.send(self.chunk(model, json!([{
            "index": 0,
            "delta": { "tool_calls": [{
                "index": 0,
                "id": format!("call_{}", comp_name),
                "type": comp_type.clone(),
                "icon_svg": icon_svg.clone(),
                "function": {
                    "name": public_call_name.clone(),
                    "sub_function": tool_sub_func.clone(),
                    "arguments": payload_str,
                    "icon_svg": icon_svg.clone()
                }
            }]}
        }])));

        let tool_start_time = std::time::Instant::now();
        let sec_mode = cluaiz_agents::ToolsEngine::get_tool(&comp_name)
            .ok()
            .flatten()
            .map(|t| t.security_mode)
            .unwrap_or(cluaiz_agents::SecurityMode::Inherit);

        // Human-in-the-loop approval gate; searching other chats asks first (policy cross_chat: ask)
        let assessment = if is_meta_tool
            && super::recall::needs_cross_chat_approval(meta_name, &meta_payload, ctx.chat_id.as_deref())
        {
            super::authorize::Assessment { requires_approval: true, capabilities: vec!["memory:other_chats".to_string()] }
        } else {
            super::authorize::assess(&comp_type, &comp_name, tool_sub_func.as_deref(), is_meta_tool, ctx.chat_id.as_deref())
        };
        let mut denial: Option<String> = None;
        if assessment.requires_approval && self.nobody_answered {
            let decision = Err(super::authorize::NOBODY_ANSWERED.to_string());
            self.tool_events.push(super::authorize::approval_event(&public_call_name, &decision));
            denial = decision.err();
        } else if assessment.requires_approval {
            let pending = super::authorize::request(&self.state, &comp_type, &public_call_name, &payload_str, ctx.chat_id.as_deref()).await;
            self.send(self.chunk(model, json!([{
                "index": 0,
                "delta": { "permission_request": {
                    "request_id": pending.request_id.clone(),
                    "tool_name": public_call_name.clone(),
                    "category": comp_type.clone(),
                    "capabilities": assessment.capabilities,
                    "parameters": serde_json::from_str::<Value>(&payload_str).unwrap_or(json!(payload_str)),
                    "status": "pending_approval",
                    "timeout_seconds": super::authorize::APPROVAL_TIMEOUT_SECS
                }}
            }])));
            let decision = super::authorize::wait(&self.state, pending, &public_call_name).await;
            self.tool_events.push(super::authorize::approval_event(&public_call_name, &decision));
            self.nobody_answered = decision.as_ref().err().is_some_and(|reason| super::authorize::timed_out(reason));
            denial = decision.err();
        } else {
            self.send(self.chunk(model, json!([{
                "index": 0,
                "delta": { "tool_status": {
                    "id": format!("call_{}", comp_name),
                    "name": public_call_name.clone(),
                    "category": comp_type.clone(),
                    "status": "pending",
                    "message": "Authorized. Queued for execution."
                }}
            }])));
        }
        if ctx.cancel_signal.load(Ordering::Relaxed) {
            return CallRun::Stop;
        }

        let permission_granted = denial.is_none();
        let execution_result = if let Some(reason) = denial {
            super::authorize::denied_result(&reason)
        } else {
            self.send(self.chunk(model, json!([{
                "index": 0,
                "delta": { "tool_status": {
                    "id": format!("call_{}", comp_name),
                    "name": public_call_name.clone(),
                    "category": comp_type.clone(),
                    "status": "running",
                    "message": "Executing tool in background process"
                }}
            }])));
            let executed = if is_meta_tool {
                let outcome = super::tool_turns::run_meta_tool(&self.state, ctx, &self.tool_turns, session_id.as_deref(), meta_name, &meta_payload).await;
                if meta_name == cluaiz_agents::LOAD_TOOL_NAME && outcome.text.contains("Loaded tool") {
                    if let Some(id) = serde_json::from_str::<Value>(&meta_payload)
                        .ok()
                        .and_then(|a| a.get("id").and_then(|v| v.as_str()).map(str::to_string))
                    {
                        self.tool_turns.add_loaded_tool(ctx, &id).await;
                    }
                }
                self.tool_events.push(super::preflight::tool_event(meta_name, &meta_payload, &outcome.text));
                self.unloaded = outcome.unloaded;
                Ok(outcome.text)
            } else {
                cluaiz_agents::ToolsEngine::execute_tool_by_name(&comp_type, &comp_name, tool_sub_func.as_deref(), &payload_str)
                    .await
                    .map(|output| if comp_type == "mcp" { cluaiz_agents::ToolsEngine::mcp_text_for_model(&output) } else { output })
            };
            match executed {
                Ok(text) => {
                    tracing::info!("✅ [API] Tool '{}' done ({} chars)", public_call_name, text.len());
                    text
                }
                Err(e) => {
                    tracing::error!("❌ [API] Tool '{}' failed: {}", public_call_name, e);
                    format!("Error executing {}: {}", public_call_name, e)
                }
            }
        };
        let latency_ms = tool_start_time.elapsed().as_secs_f64() * 1000.0;

        // A project file the tool read or changed: the project's glob rules for it come with the
        // result, once in the chat (as the terminal agent does for its file tools)
        let mut model_result = execution_result.clone();
        let mut rules_applied: Vec<(String, String)> = Vec::new();
        if let (Some(workspace), true) = (&ctx.project, permission_granted && !is_meta_tool && !execution_result.starts_with("Error executing")) {
            let files = super::project::files_of_call(workspace, &cluaiz_agents::ToolsEngine::workspace_dir(), &payload_str);
            let (text, applied) = super::project::glob_rules(workspace, &files, &mut self.project_sent);
            if !applied.is_empty() {
                model_result.push_str(&text);
                if let Some((chat, _)) = &ctx.ledger_turn {
                    let mut ledger = cluaiz_agents::ChatLedger::get(chat);
                    for key in self.project_sent.iter().filter(|key| key.starts_with("rule:")) {
                        if !ledger.project_sent.contains(key) {
                            ledger.project_sent.push(key.clone());
                        }
                    }
                    cluaiz_agents::ChatLedger::put(chat, ledger);
                }
                tracing::info!("📏 [Project] glob rules for {}: {:?}", public_call_name, applied);
            }
            rules_applied = applied;
        }

        if self.streaming() {
            let status_str = if !permission_granted {
                "denied"
            } else if execution_result.starts_with("Error executing") {
                "failed"
            } else {
                "completed"
            };
            let mut execution_logs = vec![format!("[ToolsEngine] Invoking {} '{}' (security: {:?})", comp_type, public_call_name, sec_mode)];
            if let Ok(val) = serde_json::from_str::<Value>(&execution_result) {
                if let Some(cmd) = val.get("command").and_then(|v| v.as_str()) {
                    execution_logs.push(format!("[Process] Command: {}", cmd));
                }
                if let Some(exit_code) = val.get("exit_code") {
                    execution_logs.push(format!("[Process] Exit Code: {}", exit_code));
                }
                if let Some(stdout) = val.get("stdout").and_then(|v| v.as_str()) {
                    if !stdout.trim().is_empty() {
                        execution_logs.push(format!("[Process] Output captured ({} bytes)", stdout.len()));
                    }
                }
            }
            execution_logs.push(format!("[ToolsEngine] Execution {} in {:.2}ms", status_str, latency_ms));
            self.send(self.chunk(model, json!([{
                "index": 0,
                "delta": { "tool_result": {
                    "id": format!("call_{}", comp_name),
                    "name": public_call_name,
                    "category": comp_type,
                    "icon_svg": icon_svg,
                    "status": status_str,
                    "security_mode": sec_mode.as_str(),
                    "latency_ms": ((latency_ms * 100.0).round() / 100.0),
                    "input_payload": serde_json::from_str::<Value>(&payload_str).unwrap_or(json!(payload_str)),
                    "output_result": serde_json::from_str::<Value>(&execution_result).unwrap_or(json!(&execution_result)),
                    "logs": execution_logs,
                    "result": execution_result.clone(),
                    "rules": rules_applied.iter().map(|(name, path)| json!({ "name": name, "path": path })).collect::<Vec<_>>()
                }}
            }])));
        }
        CallRun::Done(model_result)
    }

    fn record_answer(&mut self, _said: &str, _calls: &[ToolCall]) {
        // The continuation renders each call from its raw text with its result (record_result)
    }

    fn record_result(&mut self, call: &ToolCall, result: &str) {
        // The call and its result join the conversation once; the next step renders it again. An
        // unloaded tool's earlier load result in this answer becomes a short note.
        if let Some(id) = self.unloaded.take() {
            self.tool_turns.tombstone(&id);
        }
        self.tool_turns.push(&call.raw, &call.name, result);
    }

    fn record_correction(&mut self, _said: &str, _note: &str) -> bool {
        // The continuation holds tool steps only; an unreadable call here comes back to the model
        // as the result of a call (see `call`), so the API turn makes no corrections of its own
        false
    }

    fn stopped(&self) -> bool {
        self.ctx.cancel_signal.load(Ordering::Relaxed)
            || self.sink.as_ref().is_some_and(|sink| sink.is_closed())
    }
}

/// The tool's SVG icon from its package folder
fn tool_icon(comp_type: &str, comp_name: &str) -> Option<String> {
    let env = cluaiz_core::EnvironmentManager::current();
    let tool_dir = match comp_type {
        "skill" => env.skills_dir().join(comp_name),
        "plugin" => env.plugins_dir().join(comp_name),
        "mcp" => env.mcp_dir().join(comp_name),
        _ => env.tools_dir().join(comp_name),
    };
    [tool_dir.join("assets").join("icon.svg"), tool_dir.join("icon.svg")]
        .into_iter()
        .find(|p| p.exists())
        .and_then(|p| std::fs::read_to_string(p).ok())
}

/// The API error type of a failed generation: a provider's own errors are `provider_error`
/// (a full context stays `exceed_context_size_error`), the Engine's as `engine_error_type` says
fn error_kind(ctx: &super::preflight::PreparedChatContext, error: &str) -> &'static str {
    match &ctx.provider {
        Some(_) if error.contains(crate::providers::CONTEXT_FULL) => "exceed_context_size_error",
        Some(_) => "provider_error",
        None => super::types::engine_error_type(error),
    }
}
