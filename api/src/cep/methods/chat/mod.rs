//! `chat.*`: control of chats running on `/v1/chat/completions` (cancel, skip reasoning), their
//! tools and context telemetry, and the read-only import of the App's old chats
//! (03-method-catalog Part B). Owner: Claude (Wave A).

mod app_import;

use crate::cep::CallCtx;
use crate::handlers::chat::types::ACTIVE_STREAMS;
use cluaiz_agents::{SessionToolBinding, ToolsEngine};
use cluaiz_cep::{Call, CepError, Meta, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Stream {
    /// The `id` of the running chat stream (from its SSE chunks)
    stream_id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Telemetry {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    chat_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChatId {
    chat_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetTools {
    chat_id: String,
    /// Tools to attach or update: a tool id (its default turns), or `{"id", "turns"}`
    #[serde(default)]
    tools: Vec<Value>,
    /// Tool ids to detach
    #[serde(default)]
    detach: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportApp {
    /// One App chat id (its folder name); leave out with `all: true` for every chat
    #[serde(default)]
    chat_id: Option<String>,
    #[serde(default)]
    all: bool,
    /// Count what would be imported, write nothing
    #[serde(default)]
    dry_run: bool,
}

/// A tool to attach: `{"id", "turns", …}`, or just its id with the tool's default turns
#[derive(Deserialize)]
#[serde(untagged)]
enum ToolInput {
    Binding(SessionToolBinding),
    Id(String),
}

impl ToolInput {
    fn into_binding(self) -> SessionToolBinding {
        match self {
            Self::Binding(binding) => binding,
            Self::Id(id) => {
                let turns = ToolsEngine::get_tool(&id).ok().flatten().map(|tool| tool.default_turns).unwrap_or(-1);
                SessionToolBinding { id, turns, inline: false, pinned: false, idle: 0 }
            }
        }
    }
}

fn stream_not_found(id: &str) -> CepError {
    CepError::not_found("stream_not_found", format!("Active stream '{id}' not found or already completed"))
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "chat.cancel",
            Meta::new(Scope::Write, "Stops a running chat stream").example(
                json!({ "stream_id": "chatcmpl-1a2b" }),
                json!({ "stream_id": "chatcmpl-1a2b", "status": "cancelled" }),
            ),
            |_: CallCtx, _: Call, p: Stream| async move {
                let found = ACTIVE_STREAMS.read().ok().and_then(|streams| streams.get(&p.stream_id).map(|entry| entry.cancel.store(true, Ordering::Relaxed)));
                if found.is_none() {
                    return Err(stream_not_found(&p.stream_id));
                }
                // Also interrupt native generation between its steps
                cluaiz_core::GLOBAL_CANCEL_SIGNAL.store(true, Ordering::SeqCst);
                tracing::info!("🛑 [StreamControl] Cancel for stream '{}' ({})", p.stream_id, p.reason.as_deref().unwrap_or("no reason given"));
                Ok(json!({ "stream_id": p.stream_id, "status": "cancelled" }))
            },
        )
        .add(
            "chat.skip_reasoning",
            Meta::new(Scope::Write, "Ends the model's reasoning; the same stream continues with the answer").example(
                json!({ "stream_id": "chatcmpl-1a2b" }),
                json!({ "stream_id": "chatcmpl-1a2b", "status": "skip_requested" }),
            ),
            |_: CallCtx, _: Call, p: Stream| async move {
                let entry = ACTIVE_STREAMS
                    .read()
                    .map_err(|_| CepError::unavailable("stream_control_unavailable", "Stream controls unavailable"))?
                    .get(&p.stream_id)
                    .cloned()
                    .ok_or_else(|| stream_not_found(&p.stream_id))?;
                if !entry.reasoning_supported {
                    return Err(CepError::conflict("reasoning_unavailable", "This stream has no enabled native reasoning control"));
                }
                entry.skip_reasoning.store(true, Ordering::Release);
                Ok(json!({ "stream_id": p.stream_id, "status": "skip_requested" }))
            },
        )
        .add(
            "chat.telemetry",
            Meta::new(Scope::Read, "How a chat's context is used by its tools, for a model").example(
                json!({ "model": "qwen3-4b-gguf-Q4_K_M", "chat_id": "c1" }),
                json!({ "model": "qwen3-4b-gguf-Q4_K_M", "chat_id": "c1", "telemetry": {} }),
            ),
            |_: CallCtx, _: Call, p: Telemetry| async move {
                let model = p.model.unwrap_or_else(|| "default".to_string());
                let chat = p.chat_id.unwrap_or_else(|| "default".to_string());
                let active = ToolsEngine::get_active_tool_ids_for_session(&chat);
                let telemetry = ToolsEngine::compute_telemetry(&model, &chat, &active, 0, 0, 0, 0);
                Ok(json!({ "model": model, "chat_id": chat, "telemetry": telemetry }))
            },
        )
        .add(
            "chat.tools",
            Meta::new(Scope::Read, "The tools attached to a chat, with their turns left").example(
                json!({ "chat_id": "c1" }),
                json!({ "chat_id": "c1", "active_tools": [{ "id": "web-search", "turns": 3 }] }),
            ),
            |_: CallCtx, _: Call, p: ChatId| async move {
                Ok(json!({ "chat_id": p.chat_id, "active_tools": ToolsEngine::get_session_tools(&p.chat_id) }))
            },
        )
        .add(
            "chat.set_tools",
            Meta::new(Scope::Write, "Attaches, updates or detaches a chat's tools").example(
                json!({ "chat_id": "c1", "tools": ["web-search", { "id": "calc", "turns": 2 }], "detach": ["old-tool"] }),
                json!({ "chat_id": "c1", "active_tools": [{ "id": "web-search", "turns": 3 }] }),
            ),
            |_: CallCtx, _: Call, p: SetTools| async move {
                let mut tools = Vec::with_capacity(p.tools.len());
                for (i, value) in p.tools.into_iter().enumerate() {
                    let input: ToolInput = serde_json::from_value(value).map_err(|e| {
                        CepError::invalid_params(format!("tools[{i}]: {e}"), vec![json!({ "path": format!("tools[{i}]"), "message": e.to_string() })], None)
                    })?;
                    tools.push(input.into_binding());
                }
                let updated = ToolsEngine::update_session_tools(&p.chat_id, tools, p.detach);
                Ok(json!({ "chat_id": p.chat_id, "active_tools": updated }))
            },
        )
        .add(
            "chat.import_app",
            Meta::new(Scope::Write, "Imports the App's old chats (read-only) into the Engine's event logs, as a task")
                .task()
                .no_mcp()
                .example(json!({ "all": true, "dry_run": true }), json!({ "task": { "task_id": "t_5f0c…", "status": "working" } })),
            |_: CallCtx, call: Call, p: ImportApp| async move {
                if p.chat_id.is_none() && !p.all {
                    return Err(CepError::invalid_params("Send `chat_id`, or `all: true`", Vec::new(), Some(json!({ "all": true }))));
                }
                call.start_task(move |task| async move {
                    tokio::task::spawn_blocking(move || app_import::import(p.chat_id.as_deref(), p.dry_run, &task))
                        .await
                        .map_err(|_| CepError::internal("chat.import_app"))
                })
            },
        );
}
