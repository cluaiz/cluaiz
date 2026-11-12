//! A provider's model as the model of the Engine's own chat (`/v1/chat/completions` with memory,
//! the chat's ledger, Engine tools, context planning and stream controls). The Engine renders the
//! conversation as for an installed model; `ProviderRun` sends it to the provider and streams the
//! answer back as tokens in the Engine's form: thinking between `<think>` tags, calls as
//! `<tool_call>` blocks. `open` is shared with the terminal (`cmd/src/chat/provider.rs`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cluaiz_core::providers::Target;
use serde_json::{json, Value};

use super::compat::Compat;
use super::errors::{
    classify, error_message, refused_calls, refused_field, send_error, thinking_required,
    tool_call_refused, Failure,
};
use super::history::{drop_oldest_turn, openai_conversation, openai_conversation_with_parts};
use super::request::{chat_body, drop_thinking_off, ChatParts, Sampling, Thinking, OPTIONAL_FIELDS};
use super::stream::{data_of, sse_lines, Decoder, Read, THINK_END, THINK_START};
use super::{http, retry};

/// Most turns taken out of one request before the context error is returned as it is
const MOST_DROPS: usize = 16;

/// Sent once when a provider refused a call to a function it was not given
const TOOLS_REMINDER: &str = "Call only the functions listed in tools. To use another installed tool, call load_tool with its id first; its functions are then listed.";

/// What sending a chat request gave
pub enum Opened {
    /// The provider streams its answer
    Stream {
        response: reqwest::Response,
        key: Option<String>,
    },
    /// The provider refused a call to a function it was not given and named the call: it goes
    /// on to the Engine as the answer (`<tool_call>` blocks), which answers it as for an
    /// installed model
    Calls(String),
}

/// How `open` may change a request that fails
#[derive(Debug, Clone, Copy)]
pub struct Room {
    /// Take out the oldest turns when the provider says the request does not fit
    pub drop_turns: bool,
}

/// What `open` changed in the request
#[derive(Debug, Default, Clone)]
pub struct Opening {
    /// Messages taken out to fit the context
    pub dropped: usize,
    /// Optional fields the provider refused, left out
    pub left_out: Vec<String>,
    /// The model refused thinking off (it always thinks); the request went without it
    pub thinks_always: bool,
}

/// Sends a chat request until it is answered or cannot be: a rate limit or a busy provider is
/// waited for, a connection that did not open is tried once more, a field the provider does not
/// take is left out, a request that does not fit loses its oldest turns (`room.drop_turns`), and a
/// refused call is passed on. Every wait ends when `cancel` is set.
pub async fn open(
    target: &Target,
    body: &mut Value,
    room: Room,
    cancel: Option<&AtomicBool>,
) -> Result<(Opened, Opening), String> {
    let mut opening = Opening::default();
    let mut turns_dropped = 0usize;
    let mut reminded = false;
    let mut retries = 0u32;
    let mut send_retries = 0u32;
    loop {
        let (builder, key) = http::chat_request(target).await?;
        let response = match http::send(builder.body(body.to_string()), &target.config).await {
            Ok(response) => response,
            Err(error) => {
                send_retries += 1;
                if let Some(wait) = retry::wait_after_send_error(&error, send_retries) {
                    tracing::warn!(
                        "🔁 [Provider] {} did not answer ({}); trying once more",
                        target.provider,
                        send_error(target, &error, key.as_deref())
                    );
                    if retry::pause(wait, cancel).await {
                        continue;
                    }
                }
                return Err(send_error(target, &error, key.as_deref()));
            }
        };
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            return Ok((Opened::Stream { response, key }, opening));
        }
        let header_wait = retry::retry_after(response.headers());
        let text = response.text().await.unwrap_or_default();
        let wait_hint = header_wait.or_else(|| retry::retry_after_text(&text));
        let message = error_message(target, status, &text, key.as_deref());
        let failure = classify(status, &text);

        if failure == Failure::ContextFull && room.drop_turns && turns_dropped < MOST_DROPS {
            let removed = body
                .get_mut("messages")
                .and_then(Value::as_array_mut)
                .map(drop_oldest_turn)
                .unwrap_or(0);
            if removed > 0 {
                opening.dropped += removed;
                turns_dropped += 1;
                continue;
            }
        }
        // The model called a function that is not in `tools` (such as a suggested tool it did
        // not load), and the provider refused it: the call it names is passed on; without one,
        // the request is sent once more with a reminder of how tools are loaded.
        if status == 400 && tool_call_refused(&text) {
            if let Some(calls) = refused_calls(&text) {
                return Ok((Opened::Calls(calls), opening));
            }
            if !reminded {
                reminded = true;
                if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
                    messages.push(json!({ "role": "system", "content": TOOLS_REMINDER }));
                }
                continue;
            }
        }
        // Claude needs `max_tokens`; a model with a lower limit than the Engine's default gets
        // the request again with a smaller one
        let max_tokens = body["max_tokens"].as_u64().unwrap_or(0) as usize;
        if status == 400
            && text.contains("max_tokens")
            && max_tokens > super::anthropic::SMALL_MAX_TOKENS
            && Compat::wire_of(&target.config) == super::compat::Wire::Anthropic
        {
            body["max_tokens"] = json!(super::anthropic::SMALL_MAX_TOKENS);
            continue;
        }
        // A model that cannot stop thinking refused a request that asked it to: sent again
        // without that ask; its thinking is shown as it comes
        if thinking_required(status, &text) && drop_thinking_off(body) {
            tracing::warn!("🧠 [Provider] {} thinks always ({}); asked again without thinking off", target.full_name(), message);
            opening.thinks_always = true;
            continue;
        }
        // A field the Engine added on its own that this provider does not take
        let sent: Vec<&str> = OPTIONAL_FIELDS
            .iter()
            .copied()
            .filter(|f| body.get(*f).is_some())
            .collect();
        if let Some(field) = refused_field(status, &text, &sent) {
            tracing::warn!("🧩 [Provider] {} does not take `{field}`; sent again without it (set `compat` in providers.json to say so ahead)", target.provider);
            if let Some(fields) = body.as_object_mut() {
                fields.remove(field);
            }
            opening.left_out.push(field.to_string());
            continue;
        }
        retries += 1;
        if let Some(wait) = retry::wait_after(failure, retries, wait_hint) {
            tracing::warn!(
                "🔁 [Provider] {message}; trying again in {:.1}s",
                wait.as_secs_f32()
            );
            if retry::pause(wait, cancel).await {
                continue;
            }
        }
        return Err(message);
    }
}

/// A provider's model as the chat's model
pub struct ProviderRun {
    pub target: Target,
    /// The request's own `max_tokens` (the Engine's default, the whole context, is not sent)
    pub max_tokens: Option<usize>,
    /// OpenAI fields the Engine's prompt does not carry (`stop`, `logit_bias`, `user`)
    pub extra: serde_json::Map<String, Value>,
    /// What the last generation used, in the form of a loaded model's report
    pub report: Arc<std::sync::Mutex<Option<cluaiz_runtime::ContextReport>>>,
}

impl ProviderRun {
    pub fn new(
        target: Target,
        max_tokens: Option<usize>,
        extra: serde_json::Map<String, Value>,
    ) -> Self {
        Self {
            target,
            max_tokens,
            extra,
            report: Default::default(),
        }
    }

    pub fn last_report(&self) -> Option<cluaiz_runtime::ContextReport> {
        self.report.lock().ok().and_then(|report| report.clone())
    }

    /// One generation of the turn: `prompt` is the Engine's prompt (a chat envelope as JSON)
    pub async fn dispatch(
        &self,
        prompt: &str,
        think_mode: &str,
        effort: cluaiz_core::ThinkingBudget,
        cancel: Arc<AtomicBool>,
    ) -> cluaiz_runtime::HostResponse {
        use cluaiz_runtime::HostResponse;
        let envelope: Value = match serde_json::from_str(prompt) {
            Ok(envelope) => envelope,
            Err(error) => {
                return HostResponse::Error(format!(
                    "the prompt for {} is not a chat envelope: {error}",
                    self.target.full_name()
                ))
            }
        };
        let pairs: Vec<(String, String)> = envelope["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|m| {
                let content = match &m["content"] {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                (m["role"].as_str().unwrap_or("user").to_string(), content)
            })
            .collect();
        self.send(
            openai_conversation(&pairs, Some(&self.target)),
            &envelope,
            think_mode,
            effort,
            cancel,
        )
        .await
    }

    /// One generation of a turn with attachments: `prepared` holds the decoded images and sounds;
    /// `prompt` (the turn's text envelope) gives the tools and the samplers
    pub async fn dispatch_prepared(
        &self,
        prepared: cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>,
        prompt: &str,
        think_mode: &str,
        effort: cluaiz_core::ThinkingBudget,
        cancel: Arc<AtomicBool>,
    ) -> cluaiz_runtime::HostResponse {
        let envelope: Value = serde_json::from_str(prompt).unwrap_or_else(|_| json!({}));
        match openai_conversation_with_parts(prepared.messages, Some(&self.target)) {
            Ok(messages) => {
                self.send(messages, &envelope, think_mode, effort, cancel)
                    .await
            }
            Err(error) => cluaiz_runtime::HostResponse::Error(error),
        }
    }

    async fn send(
        &self,
        messages: Vec<Value>,
        envelope: &Value,
        think_mode: &str,
        effort: cluaiz_core::ThinkingBudget,
        cancel: Arc<AtomicBool>,
    ) -> cluaiz_runtime::HostResponse {
        use cluaiz_runtime::HostResponse;
        let target = self.target.clone();
        let compat = Compat::of(&target);
        let thinking = Thinking::of(think_mode, effort);
        let sampling = Sampling::from_request(&envelope["samplers"], self.max_tokens);
        let parts = ChatParts {
            model: &target.model,
            messages,
            sampling: &sampling,
            thinking,
            tools: envelope.get("tools"),
            response_format: envelope.get("response_format"),
            extra: Some(&self.extra),
        };
        let mut body = chat_body(&compat, parts);
        let room = Room { drop_turns: true };
        let (response, key, opening) = match open(&target, &mut body, room, Some(&cancel)).await {
            Ok((Opened::Stream { response, key }, opening)) => (response, key, opening),
            Ok((Opened::Calls(calls), _)) => {
                let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
                let _ = tx.try_send(calls);
                return HostResponse::TokenStream(rx);
            }
            Err(message) => return HostResponse::Error(message),
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(512);
        let report = Arc::clone(&self.report);
        let n_ctx = target.context() as usize;
        tokio::spawn(async move {
            let mut response = response;
            let mut key = key;
            let mut opening = opening;
            let mut empty_tries = 0u32;
            let mut in_think = false;
            let mut stop = "eos";
            let finished = loop {
                let mut decoder = Decoder::new(&compat);
                let mut pending: Vec<u8> = Vec::new();
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                let mut failed: Option<String> = None;
                'read: loop {
                    let chunk = tokio::select! {
                        chunk = response.chunk() => chunk,
                        _ = tick.tick() => {
                            if cancel.load(Ordering::Relaxed) || tx.is_closed() {
                                stop = "cancel";
                                break;
                            }
                            continue;
                        }
                    };
                    let chunk = match chunk {
                        Ok(Some(chunk)) => chunk,
                        Ok(None) => break,
                        Err(error) => {
                            failed = Some(send_error(&target, &error, key.as_deref()));
                            break;
                        }
                    };
                    for line in sse_lines(&mut pending, &chunk) {
                        let Some(data) = data_of(&line) else { continue };
                        match decoder.read(data) {
                            Read::Done => break 'read,
                            Read::Nothing => {}
                            Read::Error { message, raw } => {
                                // A refused call to a function not in `tools` goes on to the
                                // Engine, which answers it as for an installed model
                                if let Some(calls) = tool_call_refused(&raw)
                                    .then(|| refused_calls(&raw))
                                    .flatten()
                                {
                                    let mut text = String::new();
                                    if in_think {
                                        text.push_str(THINK_END);
                                    }
                                    text.push('\n');
                                    text.push_str(&calls);
                                    let _ = tx.send(text).await;
                                    return;
                                }
                                failed = Some(format!(
                                    "{}: {}",
                                    target.provider,
                                    cluaiz_core::providers::redact(&message, key.as_deref())
                                ));
                                break 'read;
                            }
                            // Thinking is passed on as it comes, also with thinking off (a
                            // model may think anyway); whether it is shown is the caller's
                            Read::Piece { thinking, text, .. } => {
                                let mut out = String::new();
                                if let Some(thinking) = thinking {
                                    if !in_think {
                                        out.push_str(THINK_START);
                                        in_think = true;
                                    }
                                    out.push_str(&thinking);
                                }
                                if let Some(text) = text {
                                    if in_think {
                                        out.push_str(THINK_END);
                                        in_think = false;
                                    }
                                    out.push_str(&text);
                                }
                                if !out.is_empty() && tx.send(out).await.is_err() {
                                    stop = "cancel";
                                    break 'read;
                                }
                            }
                        }
                    }
                }
                // An answer that gave nothing at all (no text, thinking or call) is asked for once
                // more, as is a stream that broke before anything came
                if !decoder.produced()
                    && stop != "cancel"
                    && empty_tries < retry::EMPTY_ANSWER_TRIES
                {
                    empty_tries += 1;
                    tracing::warn!(
                        "🔁 [Provider] {} answered nothing{}; asking once more",
                        target.provider,
                        failed
                            .as_deref()
                            .map(|f| format!(" ({f})"))
                            .unwrap_or_default()
                    );
                    match open(&target, &mut body, room, Some(&cancel)).await {
                        Ok((
                            Opened::Stream {
                                response: again,
                                key: again_key,
                            },
                            again_opening,
                        )) => {
                            response = again;
                            key = again_key;
                            opening.dropped += again_opening.dropped;
                            continue;
                        }
                        Ok((Opened::Calls(calls), _)) => {
                            let _ = tx.send(calls).await;
                            return;
                        }
                        Err(message) => {
                            let _ = tx.send(format!("[ERROR: {message}]")).await;
                            return;
                        }
                    }
                }
                if let Some(message) = failed {
                    let _ = tx.send(format!("[ERROR: {message}]")).await;
                    return;
                }
                break decoder.finish(&target.provider);
            };
            let mut tail = finished.text;
            if in_think {
                tail.push_str(THINK_END);
            }
            for block in finished.calls {
                tail.push('\n');
                tail.push_str(&block);
            }
            if !tail.is_empty() {
                let _ = tx.send(tail).await;
            }
            if finished.finish.as_deref() == Some("length") {
                stop = "limit";
            }
            let (prompt_tokens, n_tokens) = finished.usage.unwrap_or((0, 0));
            if let Ok(mut slot) = report.lock() {
                *slot = Some(cluaiz_runtime::ContextReport {
                    n_ctx,
                    prompt_tokens,
                    processed_tokens: prompt_tokens,
                    n_tokens,
                    dropped_messages: opening.dropped,
                    truncated: stop == "limit" || opening.dropped > 0,
                    stop,
                    kv_reuse: "prefix",
                    cause: "provider",
                    ..Default::default()
                });
            }
        });
        HostResponse::TokenStream(rx)
    }
}
