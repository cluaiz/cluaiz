//! `context.*`: a dry-run preview of a chat request's plan, the context reports of a chat's
//! recent requests, and the idle-time job queue. The planner's settings are `config` section
//! `context`.

use crate::cep::{error_from_response, CallCtx};
use crate::handlers::chat::{self, background, context_plan, preflight::PreparedChatContext, types::ExternalChatRequest};
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Preview {
    /// The chat the request belongs to (its event log, ledger and loaded tools are used)
    chat_id: String,
    /// A chat completions request (model, messages, tools…); its `session_id` becomes `chat_id`
    request: Value,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Reports {
    /// The chat (`session_id` of its chat requests)
    chat_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Report {
    /// The chat (`session_id` of its chat requests)
    chat_id: String,
    /// One id from `context.reports` (the chat completion's `id`)
    request_id: String,
}

pub fn register(registry: &mut Registry<CallCtx>) {
    registry
        .add(
            "context.preview",
            Meta::new(Scope::Read, "The plan and tool choices a chat request would get, without generating (loads the model for its tokenizer)")
                .error(-32602, "invalid_params", "`request` is not a chat request, or its `messages` is empty")
                .error(-32004, "model_not_found", "The request's model is not installed")
                .example(
                    json!({ "chat_id": "c1", "request": { "model": "qwen3-4b-gguf-Q4_K_M", "messages": [{ "role": "user", "content": "Hi" }] } }),
                    json!({ "plan": { "prompt_tokens": 412, "n_ctx": 8192 }, "tools": { "cards": [] } }),
                ),
            |ctx: CallCtx, _: Call, p: Preview| async move {
                let mut request: ExternalChatRequest = serde_json::from_value(p.request).map_err(|e| {
                    CepError::invalid_params(format!("`request` is not a chat request: {e}"), vec![json!({ "path": "request", "message": e.to_string() })], None)
                })?;
                if request.messages.is_empty() {
                    return Err(CepError::invalid_params("`request.messages` cannot be empty", Vec::new(), None));
                }
                request.session_id = Some(p.chat_id);
                request.preview = true;
                chat::engine_history(&mut request);
                let request_id = format!("preview-{}", uuid::Uuid::new_v4().simple());
                match PreparedChatContext::prepare(&ctx.state, &request, &request_id, None).await {
                    Ok(prepared) => Ok(json!({ "plan": prepared.plan_report, "tools": prepared.tools_report })),
                    Err(response) => Err(error_from_response(response).await),
                }
            },
        )
        .add(
            "context.reports",
            Meta::new(Scope::Read, "Ids of a chat's kept context reports, newest last")
                .example(json!({ "chat_id": "c1" }), json!({ "chat_id": "c1", "reports": ["chatcmpl-1a2b"] })),
            |_: CallCtx, _: Call, p: Reports| async move {
                Ok(json!({ "chat_id": p.chat_id, "reports": context_plan::recall(&p.chat_id, None).unwrap_or(json!([])) }))
            },
        )
        .add(
            "context.report",
            Meta::new(Scope::Read, "One of a chat's last 20 context reports (kept in memory)")
                .error(-32004, "report_not_found", "No such report: only the newest 20 per chat are kept, in memory, until a restart")
                .example(
                json!({ "chat_id": "c1", "request_id": "chatcmpl-1a2b" }),
                json!({ "chat_id": "c1", "request_id": "chatcmpl-1a2b", "context_report": { "timing": { "preflight_ms": 23 } } }),
            ),
            |_: CallCtx, _: Call, p: Report| async move {
                let report = context_plan::recall(&p.chat_id, Some(&p.request_id))
                    .ok_or_else(|| CepError::not_found("report_not_found", "No such report (the newest 20 per chat are kept, in memory)"))?;
                Ok(json!({ "chat_id": p.chat_id, "request_id": p.request_id, "context_report": report }))
            },
        )
        .add(
            "context.jobs",
            Meta::new(Scope::Read, "The idle-time scheduler's queue, counts and recent results")
                .example(json!({}), json!({ "indexer": "auto", "requests_in_flight": 0, "running": false,
                    "queue": [{ "chat_id": "c1", "answer_pos": 3, "kind": "cold" }],
                    "counts": { "done": 12, "failed": 0, "stopped": 0, "skipped": 0 },
                    "recent": [{ "chat": "c2", "answer_pos": 5, "cold": false, "outcome": "done", "ms": 8017 }] })),
            |_: CallCtx, _: Call, _: NoParams| async move { Ok(background::status()) },
        );
}
