//! Calls to model providers (`config/providers.json`, `cluaiz_core::providers`), shared by the
//! terminal commands, the CEP `providers.*` methods and `/v1/chat/completions`.
//!
//! The key goes only into the `Authorization` header of a request to the provider's own URL. Every
//! message that could hold it passes through `redact` first.

use std::time::Duration;

use cluaiz_core::providers::{redact, Provider, Target};
use serde_json::{json, Value};

/// Fields of an OpenAI chat request a provider receives; the Engine's own fields stay here
const OPENAI_FIELDS: &[&str] = &[
    "model", "messages", "stream", "stream_options", "temperature", "top_p", "max_tokens", "max_completion_tokens",
    "stop", "seed", "presence_penalty", "frequency_penalty", "response_format", "tools", "tool_choice",
    "parallel_tool_calls", "logit_bias", "logprobs", "top_logprobs", "n", "user", "reasoning_effort",
];

/// One client for every provider call: it keeps connections open between requests
pub fn client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .user_agent(concat!("cluaiz/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

/// A request to the provider with its key, when it has one
pub fn request(provider: &Provider, method: reqwest::Method, url: &str) -> Result<(reqwest::RequestBuilder, Option<String>), String> {
    let key = provider.key()?;
    let mut builder = client().request(method, url);
    if let Some(key) = &key {
        builder = builder.bearer_auth(key);
    }
    Ok((builder, key))
}

/// The chat request a provider receives: only OpenAI fields, with the provider's model id
pub fn chat_body(target: &Target, request: &Value) -> Value {
    let mut body = serde_json::Map::new();
    if let Some(fields) = request.as_object() {
        for (name, value) in fields {
            if OPENAI_FIELDS.contains(&name.as_str()) && !value.is_null() {
                body.insert(name.clone(), value.clone());
            }
        }
    }
    body.insert("model".into(), json!(target.model));
    // The Engine's thinking levels are words or a token count; a provider takes only low/medium/high
    if body.get("reasoning_effort").is_some_and(|v| !matches!(v.as_str(), Some("low" | "medium" | "high"))) {
        body.remove("reasoning_effort");
    }
    Value::Object(body)
}

/// A failed provider answer in plain words. A context overflow says "exceeds the available
/// context", as the Engine's own error does, so callers that make room for it work the same.
pub fn error_message(target: &Target, status: u16, body: &str, key: Option<&str>) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            let error = v.get("error")?;
            error.get("message").and_then(Value::as_str).map(str::to_owned).or_else(|| error.as_str().map(str::to_owned))
        })
        .unwrap_or_else(|| body.chars().take(400).collect());
    let detail = redact(detail.trim(), key);
    let lower = detail.to_lowercase();
    let name = target.full_name();
    let what = match status {
        401 | 403 => format!("{} refused the key ({status}); check it with `cluaiz provider test {}`", target.provider, target.provider),
        404 => format!("{} has no model '{}' ({status}); see `cluaiz provider models {}`", target.provider, target.model, target.provider),
        429 => format!("{} is rate limiting this key ({status}); wait, or use another model", target.provider),
        400 | 413 if lower.contains("context") || lower.contains("too long") || lower.contains("too large") || lower.contains("reduce the length") => {
            format!("the request {CONTEXT_FULL} of {name} ({status})")
        }
        _ => format!("{} answered {status}", target.provider),
    };
    if detail.is_empty() {
        what
    } else {
        format!("{what}: {detail}")
    }
}

/// A connection error without the key (reqwest names only the URL)
pub fn send_error(target: &Target, error: &reqwest::Error, key: Option<&str>) -> String {
    redact(&format!("{} did not answer at {}: {error}", target.provider, target.config.base_url), key)
}

/// A model a provider offers, with the context size it reports
#[derive(Debug, Clone, PartialEq)]
pub struct OfferedModel {
    pub id: String,
    pub context: Option<u32>,
}

/// `GET {base_url}/models`: the models the provider offers. Context sizes come from the fields
/// providers use (`context_length`, `context_window` and others).
pub async fn models(name: &str, provider: &Provider) -> Result<Vec<OfferedModel>, String> {
    let target = Target { provider: name.to_string(), config: provider.clone(), model: String::new() };
    let (builder, key) = request(provider, reqwest::Method::GET, &provider.models_url())?;
    let response = builder.timeout(Duration::from_secs(30)).send().await.map_err(|e| send_error(&target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(&target, status, &text, key.as_deref()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{name}: the model list is not JSON: {e}"))?;
    Ok(parse_models(&value))
}

fn parse_models(value: &Value) -> Vec<OfferedModel> {
    let list = value.get("data").or_else(|| value.get("models")).and_then(Value::as_array).cloned().unwrap_or_default();
    let mut models: Vec<OfferedModel> = list
        .iter()
        .filter_map(|m| {
            let id = m.get("id").or_else(|| m.get("name")).and_then(Value::as_str)?.to_string();
            let context = ["context_length", "context_window", "max_context_length", "max_model_len", "context_size"]
                .iter()
                .find_map(|field| m.get(*field).and_then(Value::as_u64))
                .or_else(|| m.pointer("/top_provider/context_length").and_then(Value::as_u64))
                .map(|n| n.min(u32::MAX as u64) as u32);
            Some(OfferedModel { id, context })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

/// One short request of the model's kind (providers.json; chat when it has none): the key, the
/// URL and the model work. Returns what came back, in a few words.
pub async fn test(target: &Target) -> Result<String, String> {
    match target.listed_kind() {
        Some("embedding") => test_other(target, "/embeddings", json!({ "model": target.model, "input": "ready" }), |body| {
            let size = serde_json::from_slice::<Value>(body).ok()?.pointer("/data/0/embedding")?.as_array()?.len();
            Some(format!("a vector of {size} numbers"))
        })
        .await,
        Some("tts") => test_other(target, "/audio/speech", json!({ "model": target.model, "input": "ready", "response_format": "wav" }), |body| {
            Some(format!("{} bytes of audio", body.len()))
        })
        .await,
        Some("stt") => Err(format!("{} is a speech-to-text model: test it by sending audio to /v1/audio/transcriptions", target.full_name())),
        _ => test_chat(target).await,
    }
}

async fn test_other(target: &Target, path: &str, body: Value, describe: impl Fn(&[u8]) -> Option<String>) -> Result<String, String> {
    let url = format!("{}{path}", target.config.base_url.trim_end_matches('/'));
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url)?;
    let response = builder
        .timeout(Duration::from_secs(60))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| send_error(target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(target, status, &String::from_utf8_lossy(&bytes), key.as_deref()));
    }
    describe(&bytes).ok_or_else(|| format!("{}: the answer is not what {path} returns", target.provider))
}

async fn test_chat(target: &Target) -> Result<String, String> {
    let body = json!({
        "model": target.model,
        "messages": [{ "role": "user", "content": "Reply with the single word: ready" }],
        "max_tokens": 64,
        "stream": false,
    });
    let (builder, key) = request(&target.config, reqwest::Method::POST, &target.config.chat_url())?;
    let response = builder
        .timeout(Duration::from_secs(60))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| send_error(target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(target, status, &text, key.as_deref()));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: the answer is not JSON: {e}", target.provider))?;
    let message = &value["choices"][0]["message"];
    let answer = message["content"].as_str().filter(|s| !s.trim().is_empty()).or_else(|| message["reasoning_content"].as_str()).or_else(|| message["reasoning"].as_str()).unwrap_or_default();
    Ok(answer.trim().chars().take(200).collect())
}

/// One sentence when providers are set up but API authentication is off: anyone who can reach the
/// server can then use the providers' keys. The owner decides; the Engine only says so.
pub fn open_server_warning(auth_required: bool) -> Option<String> {
    if auth_required {
        return None;
    }
    let providers = cluaiz_core::providers::Providers::load_or_empty();
    let using_keys = providers.providers.values().any(|p| p.api_key_env.is_some() || p.api_key.is_some());
    using_keys.then(|| {
        "API authentication is off: anyone who can reach this server can use your provider keys. That is fine on your own computer; on a shared network or a server, turn it on with `cluaiz token create`.".to_string()
    })
}

/// The provider model a chat request names: `model` in providers.json, or, for no model or "auto",
/// the default chat model when that is a provider's. None when no provider is set up.
pub fn target_of(request: &Value) -> Option<Target> {
    let providers = cluaiz_core::providers::Providers::load_or_empty();
    if providers.providers.is_empty() {
        return None;
    }
    let installed = |name: &str| cluaiz_models::registry::InstalledStateRegistry::load().installed_models.contains_key(name);
    match request.get("model").and_then(Value::as_str).filter(|m| !m.is_empty() && *m != "auto") {
        Some(name) => providers.find(name).filter(|_| !installed(name)),
        None => {
            let active = cluaiz_security::PermissionSchema::load().get_active_chat_model()?;
            providers.find(&active).filter(|_| !installed(&active))
        }
    }
}

/// Fields of the request a provider does not take (the Engine's own, such as `session_id` or
/// `think_mode`): they are left out and named in the `X-Cluaiz-Ignored` header
pub fn ignored_fields(request: &Value) -> Vec<String> {
    let mut ignored: Vec<String> = request
        .as_object()
        .map(|fields| fields.iter().filter(|(name, value)| !value.is_null() && !OPENAI_FIELDS.contains(&name.as_str())).map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    if request.get("reasoning_effort").is_some_and(|v| !v.is_null() && !matches!(v.as_str(), Some("low" | "medium" | "high"))) {
        ignored.push("reasoning_effort".into());
    }
    ignored.sort();
    ignored
}

/// The sampler settings of settings.json, for the fields the request does not set (as a request
/// to an installed model gets them). top_k, min_p and repetition_penalty are not OpenAI fields.
pub fn add_sampler_defaults(body: &mut Value, samplers: &cluaiz_core::SamplerDefaults) {
    let defaults = [
        ("temperature", samplers.temp.map(|v| json!(v))),
        ("top_p", samplers.top_p.map(|v| json!(v))),
        ("presence_penalty", samplers.presence_penalty.map(|v| json!(v))),
        ("frequency_penalty", samplers.frequency_penalty.map(|v| json!(v))),
        ("seed", samplers.seed.map(|v| json!(v))),
    ];
    for (name, value) in defaults {
        if let (Some(value), Some(fields)) = (value, body.as_object_mut()) {
            fields.entry(name).or_insert(value);
        }
    }
}

/// Takes out the oldest turn (a user message and every message after it up to the next user
/// message); system messages at the start stay. Returns how many messages went: 0 when only the
/// last turn is left, which is never taken out.
pub fn drop_oldest_turn(messages: &mut Vec<Value>) -> usize {
    let is_user = |m: &Value| m.get("role").and_then(Value::as_str) == Some("user");
    let Some(first) = messages.iter().position(is_user) else { return 0 };
    let Some(next) = messages.iter().skip(first + 1).position(is_user).map(|at| at + first + 1) else { return 0 };
    messages.drain(first..next);
    next - first
}

/// Most turns taken out of one request before the context error is returned as it is
const MOST_DROPS: usize = 16;

/// The words every context overflow carries, from the Engine and from a provider
pub const CONTEXT_FULL: &str = "exceeds the available context";

/// `/v1/chat/completions` for a provider's model. The request's OpenAI fields go to the provider,
/// with settings.json's samplers for those it leaves out; when it does not fit the provider's
/// context, the oldest turns are left out, as for an installed model. A stream gets this request's
/// id, so `chat.cancel` stops it, and `model` is the full name in the answer.
///
/// Headers: `X-Cluaiz-Provider`, `X-Cluaiz-Ignored` (the Engine fields left out),
/// `X-Cluaiz-Dropped-Messages` (messages left out to fit).
pub async fn forward(target: Target, request: Value, request_id: String) -> axum::response::Response {
    use axum::response::IntoResponse;
    let fail = |status: u16, message: String| {
        let status = axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
        (status, axum::Json(json!({ "error": { "message": message, "type": "provider_error", "provider": target.provider } }))).into_response()
    };
    if let Err(message) = target.check_kind("chat") {
        return fail(400, message);
    }
    let ignored = ignored_fields(&request);
    if !ignored.is_empty() {
        tracing::info!("{}: fields a provider does not take were left out: {}", target.full_name(), ignored.join(", "));
    }
    let mut body = chat_body(&target, &request);
    add_sampler_defaults(&mut body, &cluaiz_core::Settings::current().samplers);
    let streaming = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let mut dropped = 0usize;
    let mut turns_dropped = 0usize;
    let (response, key) = loop {
        let (builder, key) = match request_for_chat(&target) {
            Ok(found) => found,
            Err(message) => return fail(400, message),
        };
        let response = match builder.body(body.to_string()).send().await {
            Ok(response) => response,
            Err(error) => return fail(502, send_error(&target, &error, key.as_deref())),
        };
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            break (response, key);
        }
        let text = response.text().await.unwrap_or_default();
        let message = error_message(&target, status, &text, key.as_deref());
        if message.contains(CONTEXT_FULL) && turns_dropped < MOST_DROPS {
            let removed = body.get_mut("messages").and_then(Value::as_array_mut).map(drop_oldest_turn).unwrap_or(0);
            if removed > 0 {
                dropped += removed;
                turns_dropped += 1;
                tracing::info!("{}: the request did not fit; {dropped} oldest messages left out", target.full_name());
                continue;
            }
        }
        return fail(status, message);
    };
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(if streaming { "text/event-stream" } else { "application/json" })
        .to_string();
    let mut reply = axum::response::Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, content_type.clone())
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .header("x-cluaiz-provider", target.provider.clone());
    if !ignored.is_empty() {
        reply = reply.header("x-cluaiz-ignored", ignored.join(","));
    }
    if dropped > 0 {
        reply = reply.header("x-cluaiz-dropped-messages", dropped.to_string());
    }
    let full_name = target.full_name();
    if !streaming || !content_type.contains("event-stream") {
        let text = response.text().await.unwrap_or_default();
        let text = match serde_json::from_str::<Value>(&text) {
            Ok(mut value) if value.is_object() => {
                value["model"] = json!(full_name);
                value.to_string()
            }
            _ => text,
        };
        return reply.body(axum::body::Body::from(text)).unwrap_or_else(|_| fail(500, "the provider's answer could not be passed on".into()));
    }

    // chat.cancel finds the stream by this request's id; chat.skip_reasoning says it cannot
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Ok(mut streams) = crate::handlers::chat::types::ACTIVE_STREAMS.write() {
        streams.insert(
            request_id.clone(),
            crate::handlers::chat::types::StreamSignals { cancel: cancel.clone(), skip_reasoning: Default::default(), reasoning_supported: false },
        );
    }
    let mut response = response;
    let stream = async_stream::stream! {
        let _registered = Registered(request_id.clone());
        let mut pending = String::new();
        let mut tick = tokio::time::interval(Duration::from_millis(150));
        loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => chunk,
                _ = tick.tick() => {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                        // Dropping the provider's stream ends the request there
                        yield Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"data: [DONE]\n\n"));
                        break;
                    }
                    continue;
                }
            };
            match chunk {
                Ok(Some(chunk)) => {
                    pending.push_str(&String::from_utf8_lossy(&chunk));
                    let mut out = String::new();
                    while let Some(end) = pending.find('\n') {
                        let line: String = pending.drain(..=end).collect();
                        out.push_str(&renamed_line(&line, &request_id, &full_name));
                    }
                    if !out.is_empty() {
                        yield Ok(axum::body::Bytes::from(out));
                    }
                }
                Ok(None) => {
                    if !pending.is_empty() {
                        yield Ok(axum::body::Bytes::from(renamed_line(&pending, &request_id, &full_name)));
                    }
                    break;
                }
                Err(error) => {
                    tracing::warn!("provider stream ended early: {}", redact(&error.to_string(), key.as_deref()));
                    break;
                }
            }
        }
    };
    reply.body(axum::body::Body::from_stream(stream)).unwrap_or_else(|_| fail(500, "the provider's answer could not be passed on".into()))
}

/// The stream's entry in the registry `chat.cancel` reads, taken out when the stream ends or the
/// client goes away
struct Registered(String);

impl Drop for Registered {
    fn drop(&mut self) {
        if let Ok(mut streams) = crate::handlers::chat::types::ACTIVE_STREAMS.write() {
            streams.remove(&self.0);
        }
    }
}

/// One line of the provider's stream with this request's id and the model's full name
fn renamed_line(line: &str, request_id: &str, full_name: &str) -> String {
    let Some(data) = line.trim_end().strip_prefix("data:").map(str::trim) else { return line.to_string() };
    match serde_json::from_str::<Value>(data) {
        Ok(mut event) if event.is_object() => {
            event["id"] = json!(request_id);
            event["model"] = json!(full_name);
            format!("data: {event}\n")
        }
        _ => line.to_string(),
    }
}

fn request_for_chat(target: &Target) -> Result<(reqwest::RequestBuilder, Option<String>), String> {
    let (builder, key) = request(&target.config, reqwest::Method::POST, &target.config.chat_url())?;
    Ok((builder.timeout(Duration::from_secs(24 * 3600)).header(reqwest::header::CONTENT_TYPE, "application/json"), key))
}

/// A provider's model named in a CEP call that works on installed models only: a clear error
/// instead of "not found"
pub fn refuse_provider_model(model: &str, what: &str) -> Result<(), cluaiz_cep::CepError> {
    let Some(target) = target_of(&json!({ "model": model })) else { return Ok(()) };
    Err(cluaiz_cep::CepError::conflict(
        "provider_model",
        format!("{} runs at the provider {}: {what} works on installed models only. Use it in /v1/chat/completions; providers.* manages it", target.full_name(), target.provider),
    ))
}

/// The provider model a request names in `model` (no default: embeddings and audio without a
/// model use the installed default, as before). None when it is not a provider's model.
pub fn named_target(request: &Value) -> Option<Target> {
    let name = request.get("model").and_then(Value::as_str).filter(|m| !m.is_empty() && *m != "auto")?;
    if !name.contains('/') || !crate::providers_configured() {
        return None;
    }
    target_of(request)
}

/// A JSON error in the shape of the route's own errors
fn provider_error(target: &Target, status: u16, message: String) -> axum::response::Response {
    use axum::response::IntoResponse;
    let status = axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
    (status, axum::Json(json!({ "error": { "message": message, "type": "provider_error", "provider": target.provider } }))).into_response()
}

/// One request to `{base_url}{path}`; Ok is the response with a 2xx status, Err the error reply
async fn send_to(target: &Target, path: &str, body: reqwest::Body, content_type: &str) -> Result<reqwest::Response, axum::response::Response> {
    let url = format!("{}{path}", target.config.base_url.trim_end_matches('/'));
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url).map_err(|message| provider_error(target, 400, message))?;
    let response = builder
        .timeout(Duration::from_secs(600))
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body)
        .send()
        .await
        .map_err(|error| provider_error(target, 502, send_error(target, &error, key.as_deref())))?;
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(response);
    }
    let text = response.text().await.unwrap_or_default();
    Err(provider_error(target, status, error_message(target, status, &text, key.as_deref())))
}

fn with_headers(mut reply: axum::http::response::Builder, target: &Target, ignored: &[String]) -> axum::http::response::Builder {
    reply = reply.header("x-cluaiz-provider", target.provider.clone());
    if !ignored.is_empty() {
        reply = reply.header("x-cluaiz-ignored", ignored.join(","));
    }
    reply
}

/// Fields of `request` outside `known`, named in `X-Cluaiz-Ignored`
fn ignored_of(request: &Value, known: &[&str]) -> Vec<String> {
    let mut ignored: Vec<String> = request
        .as_object()
        .map(|fields| fields.iter().filter(|(name, value)| !value.is_null() && !known.contains(&name.as_str())).map(|(name, _)| name.clone()).collect())
        .unwrap_or_default();
    ignored.sort();
    ignored
}

/// `/v1/embeddings` for a provider's model: the OpenAI request (`input`, `encoding_format`,
/// `dimensions`, `user`) goes to `{base_url}/embeddings`; the answer comes back with `model` set to
/// the full name. Engine memory and indexes keep using the installed embedding model.
pub async fn forward_embeddings(target: Target, request: Value) -> axum::response::Response {
    if let Err(message) = target.check_kind("embedding") {
        return provider_error(&target, 400, message);
    }
    const FIELDS: &[&str] = &["model", "input", "encoding_format", "dimensions", "user"];
    let ignored = ignored_of(&request, FIELDS);
    let mut body = serde_json::Map::new();
    for field in FIELDS {
        if let Some(value) = request.get(*field).filter(|v| !v.is_null()) {
            body.insert((*field).into(), value.clone());
        }
    }
    body.insert("model".into(), json!(target.model));
    let response = match send_to(&target, "/embeddings", Value::Object(body).to_string().into(), "application/json").await {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let text = response.text().await.unwrap_or_default();
    let text = match serde_json::from_str::<Value>(&text) {
        Ok(mut value) if value.is_object() => {
            value["model"] = json!(target.full_name());
            value.to_string()
        }
        _ => text,
    };
    with_headers(axum::response::Response::builder().status(200).header(axum::http::header::CONTENT_TYPE, "application/json"), &target, &ignored)
        .body(axum::body::Body::from(text))
        .unwrap_or_else(|_| provider_error(&target, 500, "the provider's answer could not be passed on".into()))
}

/// `/v1/audio/speech` for a provider's model. The provider gets the OpenAI request (`input`,
/// `voice`, `speed`, WAV audio) and the answer comes back in the Engine's own shape: `output.
/// audio_data` as a `data:audio/wav;base64,…` URI, or with `stream`, one `speech.audio.delta`
/// event and the `done` event, so an app reads both kinds of model the same way.
pub async fn forward_speech(target: Target, request: Value) -> axum::response::Response {
    if let Err(message) = target.check_kind("tts") {
        return provider_error(&target, 400, message);
    }
    let ignored = ignored_of(&request, &["model", "input", "voice", "speed", "stream"]);
    let mut body = json!({ "model": target.model, "input": request.get("input").cloned().unwrap_or(Value::Null), "response_format": "wav" });
    for field in ["voice", "speed"] {
        if let Some(value) = request.get(field).filter(|v| !v.is_null()) {
            body[field] = value.clone();
        }
    }
    let response = match send_to(&target, "/audio/speech", body.to_string().into(), "application/json").await {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let audio = match response.bytes().await {
        Ok(audio) => audio,
        Err(error) => return provider_error(&target, 502, format!("{}: the audio did not arrive: {error}", target.provider)),
    };
    use base64::Engine as _;
    let uri = format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(&audio));
    let result = json!({ "status": "success", "task": "text_to_speech", "model": target.full_name(), "info": null, "output": { "text": null, "audio_data": uri, "segments": null } });
    if request.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        let delta = json!({ "status": "chunk", "type": "speech.audio.delta", "index": 0, "format": "wav", "audio": result["output"]["audio_data"] });
        let done = json!({ "status": "done", "result": result });
        let text = format!("data: {delta}\n\ndata: {done}\n\n");
        return with_headers(axum::response::Response::builder().status(200).header(axum::http::header::CONTENT_TYPE, "text/event-stream"), &target, &ignored)
            .body(axum::body::Body::from(text))
            .unwrap_or_else(|_| provider_error(&target, 500, "the provider's answer could not be passed on".into()));
    }
    with_headers(axum::response::Response::builder().status(200).header(axum::http::header::CONTENT_TYPE, "application/json"), &target, &ignored)
        .body(axum::body::Body::from(result.to_string()))
        .unwrap_or_else(|_| provider_error(&target, 500, "the provider's answer could not be passed on".into()))
}

/// The multipart body of an OpenAI transcription request: the audio as `audio.wav` and the text
/// fields. Returns the body and its boundary.
fn multipart(fields: &[(&str, String)], wav: &[u8]) -> (Vec<u8>, String) {
    let boundary = format!("cluaiz-{}", uuid::Uuid::new_v4().simple());
    let mut body = Vec::with_capacity(wav.len() + 512);
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(wav);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, boundary)
}

/// `/v1/audio/transcriptions` for a provider's model. The audio is read as for an installed model
/// (`input_source` or `file`: a data URI, a URL or an allowed local file, any format the Engine
/// decodes), sent to the provider as WAV with `language`, `prompt` and `temperature`, and the text
/// comes back in the Engine's own shape (`output.text`, with `segments` when `timestamps` is on).
pub async fn forward_transcription(state: std::sync::Arc<crate::AppState>, target: Target, request: Value) -> axum::response::Response {
    if let Err(message) = target.check_kind("stt") {
        return provider_error(&target, 400, message);
    }
    let ignored = ignored_of(&request, &["model", "file", "input_source", "prompt", "language", "temperature", "timestamps", "stream"]);
    let source = match (request.pointer("/input_source/data").and_then(Value::as_str), request.get("file").and_then(Value::as_str)) {
        (Some(data), _) if !data.trim().is_empty() => {
            let kind = request.pointer("/input_source/source_type").and_then(Value::as_str).unwrap_or("audio").to_ascii_lowercase();
            if kind == "base64" && !data.starts_with("data:") { format!("data:application/octet-stream;base64,{data}") } else { data.to_string() }
        }
        (_, Some(file)) if !file.trim().is_empty() => file.to_string(),
        _ => return provider_error(&target, 400, "send the audio in `input_source` or `file`".into()),
    };
    let settings = cluaiz_core::Settings::current();
    let audio = match crate::utils::prepare_authorized_audio(&state, source, &settings, None).await {
        Ok(audio) => audio,
        Err((kind, message)) => return provider_error(&target, if kind == "permission_error" { 403 } else { 400 }, message),
    };
    // The provider gets one channel: the channels' mean
    let channels = audio.channels().max(1) as usize;
    let mono: Vec<f32> = audio.samples().chunks(channels).map(|frame| frame.iter().sum::<f32>() / frame.len() as f32).collect();
    let wav = match cluaiz_media::wav::encode_pcm16(&mono, audio.sample_rate(), settings.media.limits) {
        Ok(wav) => wav,
        Err(error) => return provider_error(&target, 400, format!("the audio could not be prepared: {error}")),
    };
    let timestamps = request.get("timestamps").and_then(Value::as_bool).unwrap_or(false);
    let mut fields = vec![("model", target.model.clone()), ("response_format", if timestamps { "verbose_json" } else { "json" }.to_string())];
    for field in ["language", "prompt"] {
        if let Some(value) = request.get(field).and_then(Value::as_str).filter(|v| !v.trim().is_empty() && *v != "auto") {
            fields.push((field, value.to_string()));
        }
    }
    if let Some(temperature) = request.get("temperature").and_then(Value::as_f64) {
        fields.push(("temperature", temperature.to_string()));
    }
    if timestamps {
        fields.push(("timestamp_granularities[]", "segment".into()));
    }
    let (body, boundary) = multipart(&fields, &wav);
    let response = match send_to(&target, "/audio/transcriptions", body.into(), &format!("multipart/form-data; boundary={boundary}")).await {
        Ok(response) => response,
        Err(reply) => return reply,
    };
    let answer: Value = match response.json().await {
        Ok(answer) => answer,
        Err(error) => return provider_error(&target, 502, format!("{}: the transcription is not JSON: {error}", target.provider)),
    };
    let segments = answer.get("segments").and_then(Value::as_array).map(|segments| {
        segments.iter().map(|s| json!({ "start": s["start"], "end": s["end"], "text": s["text"].as_str().map(str::trim).unwrap_or_default(), "speaker": null })).collect::<Vec<_>>()
    });
    let text = answer.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
    let result = json!({ "status": "success", "task": "speech_to_text", "model": target.full_name(), "info": null, "output": { "text": text, "audio_data": null, "segments": segments } });
    if request.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        // The provider answers at once: the whole text is the one event
        let text = format!("data: {}\n\n", json!({ "status": "done", "result": result }));
        return with_headers(axum::response::Response::builder().status(200).header(axum::http::header::CONTENT_TYPE, "text/event-stream"), &target, &ignored)
            .body(axum::body::Body::from(text))
            .unwrap_or_else(|_| provider_error(&target, 500, "the provider's answer could not be passed on".into()));
    }
    with_headers(axum::response::Response::builder().status(200).header(axum::http::header::CONTENT_TYPE, "application/json"), &target, &ignored)
        .body(axum::body::Body::from(result.to_string()))
        .unwrap_or_else(|_| provider_error(&target, 500, "the provider's answer could not be passed on".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;

    fn target() -> Target {
        Target { provider: "groq".into(), config: Provider::new("https://api.groq.com/openai/v1"), model: "qwen/qwen3-32b".into() }
    }

    #[test]
    fn the_engine_fields_stay_out_of_a_provider_request() {
        let body = chat_body(&target(), &json!({
            "model": "groq/qwen/qwen3-32b", "messages": [], "stream": true, "temperature": 0.2,
            "session_id": "abc", "think_mode": "on", "tool_search": true, "top_k": 40, "reasoning_effort": 2048, "stop": null,
        }));
        assert_eq!(body["model"], "qwen/qwen3-32b");
        assert_eq!(body["temperature"], 0.2);
        for engine_only in ["session_id", "think_mode", "tool_search", "top_k", "reasoning_effort", "stop"] {
            assert!(body.get(engine_only).is_none(), "{engine_only}");
        }
        assert_eq!(chat_body(&target(), &json!({ "reasoning_effort": "high" }))["reasoning_effort"], "high");
    }

    #[test]
    fn errors_name_the_cause_and_never_the_key() {
        let key = "gsk_secretsecretsecret1234";
        let t = target();
        let refused = error_message(&t, 401, &format!(r#"{{"error":{{"message":"Invalid API Key {key}"}}}}"#), Some(key));
        assert!(refused.contains("refused the key") && !refused.contains("secretsecret"));
        let long = error_message(&t, 400, r#"{"error":{"message":"Please reduce the length of the messages or completion."}}"#, None);
        assert!(long.contains("exceeds the available context"));
        assert!(error_message(&t, 429, "slow down", None).contains("rate limiting"));
        assert!(error_message(&t, 500, "", None).ends_with("answered 500"));
    }

    #[test]
    fn context_sizes_are_read_from_each_providers_field() {
        let models = parse_models(&json!({ "data": [
            { "id": "b", "context_window": 131072 },
            { "id": "a", "context_length": 8192 },
            { "id": "c", "top_provider": { "context_length": 4096 } },
            { "id": "d" },
        ]}));
        let ids: Vec<_> = models.iter().map(|m| (m.id.as_str(), m.context)).collect();
        assert_eq!(ids, vec![("a", Some(8192)), ("b", Some(131072)), ("c", Some(4096)), ("d", None)]);
    }

    #[test]
    fn the_oldest_turn_goes_and_the_system_prompt_and_last_turn_stay() {
        let mut messages = vec![
            json!({ "role": "system", "content": "s" }),
            json!({ "role": "user", "content": "1" }),
            json!({ "role": "assistant", "tool_calls": [] }),
            json!({ "role": "tool", "content": "r" }),
            json!({ "role": "assistant", "content": "a1" }),
            json!({ "role": "user", "content": "2" }),
        ];
        assert_eq!(drop_oldest_turn(&mut messages), 4);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["content"], "2");
        assert_eq!(drop_oldest_turn(&mut messages), 0);
    }

    #[test]
    fn engine_fields_are_named_and_saved_samplers_fill_the_gaps() {
        let request = json!({ "model": "m", "messages": [], "session_id": "c", "think_mode": "off", "reasoning_effort": "max", "top_k": null });
        assert_eq!(ignored_fields(&request), vec!["reasoning_effort", "session_id", "think_mode"]);
        let mut body = json!({ "temperature": 0.9 });
        let saved = cluaiz_core::SamplerDefaults { temp: Some(0.2), top_p: Some(0.8), top_k: Some(40), ..Default::default() };
        add_sampler_defaults(&mut body, &saved);
        assert_eq!(body["temperature"], 0.9);
        assert!((body["top_p"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert!(body.get("top_k").is_none());
    }

    #[test]
    fn stream_lines_carry_this_requests_id_and_the_full_name() {
        let line = renamed_line("data: {\"id\":\"x\",\"model\":\"m\",\"choices\":[]}\n", "chatcmpl-1", "myapi/m");
        let event: Value = serde_json::from_str(line.trim().strip_prefix("data: ").unwrap()).unwrap();
        assert_eq!(event["id"], "chatcmpl-1");
        assert_eq!(event["model"], "myapi/m");
        assert_eq!(renamed_line("data: [DONE]\n", "chatcmpl-1", "myapi/m"), "data: [DONE]\n");
        assert_eq!(renamed_line("\n", "chatcmpl-1", "myapi/m"), "\n");
    }

    #[test]
    fn a_transcription_request_is_multipart_with_the_audio_last() {
        let (body, boundary) = multipart(&[("model", "whisper".into()), ("language", "en".into())], b"RIFFdata");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with(&format!("--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwhisper\r\n")));
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\nRIFFdata\r\n"));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
        assert_eq!(ignored_of(&json!({ "model": "m", "input": "x", "keep_alive": 5, "user": null }), &["model", "input"]), vec!["keep_alive"]);
    }
}
