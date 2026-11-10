//! Calls to model providers (`config/providers.json`, `cluaiz_core::providers`), shared by the
//! terminal commands, the CEP `providers.*` methods and `/v1/chat/completions`.
//!
//! The key goes only into the `Authorization` header of a request to the provider's own URL. Every
//! message that could hold it passes through `redact` first.

use std::time::Duration;

use cluaiz_core::providers::{redact, Provider, Target};
use serde_json::{json, Value};

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
pub async fn test(target: &Target, voice: Option<&str>) -> Result<String, String> {
    match target.listed_kind() {
        Some("embedding") => test_other(target, "/embeddings", json!({ "model": target.model, "input": "ready" }), |body| {
            let size = serde_json::from_slice::<Value>(body).ok()?.pointer("/data/0/embedding")?.as_array()?.len();
            Some(format!("a vector of {size} numbers"))
        })
        .await,
        Some("tts") => {
            let mut body = json!({ "model": target.model, "input": "ready", "response_format": "wav" });
            if let Some(voice) = voice {
                body["voice"] = json!(voice);
            }
            test_other(target, "/audio/speech", body, |body| Some(format!("{} bytes of audio", body.len()))).await
        }
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

/// The provider model a chat request names (see `target_for_model`)
pub fn target_of(request: &Value) -> Option<Target> {
    target_for_model(request.get("model").and_then(Value::as_str))
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
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
        .unwrap_or_default();
    let audio = match response.bytes().await {
        Ok(audio) => audio,
        Err(error) => return provider_error(&target, 502, format!("{}: the audio did not arrive: {error}", target.provider)),
    };
    use base64::Engine as _;
    let (audio, media_type) = if audio.starts_with(b"RIFF") {
        (wav_with_sizes(&audio), "audio/wav".to_string())
    } else {
        // Not WAV after all: labelled with what the provider says it is
        (audio.to_vec(), if media_type.starts_with("audio/") { media_type } else { "application/octet-stream".into() })
    };
    let uri = format!("data:{media_type};base64,{}", base64::engine::general_purpose::STANDARD.encode(&audio));
    let result = json!({ "status": "success", "task": "text_to_speech", "model": target.full_name(), "info": null, "output": { "text": null, "audio_data": uri, "segments": null } });
    if request.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        let format = if media_type == "audio/wav" { "wav" } else { media_type.trim_start_matches("audio/") };
        let delta = json!({ "status": "chunk", "type": "speech.audio.delta", "index": 0, "format": format, "audio": result["output"]["audio_data"] });
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

/// A WAV with its RIFF and `data` sizes set from its real length. Providers that stream audio
/// write 0xFFFFFFFF there (the length is unknown when the header goes out), which players and the
/// Engine's own decoder read as a truncated file.
fn wav_with_sizes(wav: &[u8]) -> Vec<u8> {
    let mut out = wav.to_vec();
    if out.len() < 12 || &out[0..4] != b"RIFF" || &out[8..12] != b"WAVE" {
        return out;
    }
    let riff = u32::try_from(out.len() - 8).unwrap_or(u32::MAX);
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    let mut at = 12;
    while at + 8 <= out.len() {
        let size = u32::from_le_bytes([out[at + 4], out[at + 5], out[at + 6], out[at + 7]]) as usize;
        if &out[at..at + 4] == b"data" {
            let real = u32::try_from(out.len() - at - 8).unwrap_or(u32::MAX);
            if size > real as usize {
                out[at + 4..at + 8].copy_from_slice(&real.to_le_bytes());
            }
            break;
        }
        // Chunks are padded to an even length
        at += 8 + size + (size & 1);
    }
    out
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

/// The provider's model a chat names: `model`, or for no model or "auto" the default chat model
/// when that is a provider's. An installed model of the same name wins. None without providers.
pub fn target_for_model(model: Option<&str>) -> Option<Target> {
    let providers = cluaiz_core::providers::Providers::load_or_empty();
    if providers.providers.is_empty() {
        return None;
    }
    let installed = |name: &str| cluaiz_models::registry::InstalledStateRegistry::load().installed_models.contains_key(name);
    let name = match model.filter(|m| !m.trim().is_empty() && *m != "auto") {
        Some(name) => name.to_string(),
        None => cluaiz_security::PermissionSchema::load().get_active_chat_model()?,
    };
    providers.find(&name).filter(|_| !installed(&name))
}

/// A conversation in the Engine's form (`<tool_call>` blocks in assistant text, `tool` results)
/// in the OpenAI form: calls become `tool_calls`, each result answers its call by `tool_call_id`,
/// a call without a result is answered "(not run)", and a result without a call is user text.
pub fn openai_conversation(pairs: &[(String, String)]) -> Vec<Value> {
    let converted = cluaiz_core::tool_calls::openai_messages(pairs);
    let mut out: Vec<Value> = Vec::with_capacity(converted.len());
    let mut waiting: Vec<String> = Vec::new();
    let close = |out: &mut Vec<Value>, waiting: &mut Vec<String>| {
        for id in waiting.drain(..) {
            out.push(json!({ "role": "tool", "tool_call_id": id, "content": "(not run)" }));
        }
    };
    for mut message in converted {
        if message["role"] == "tool" {
            let id = message["tool_call_id"].as_str().map(str::to_owned);
            match id.and_then(|id| waiting.iter().position(|w| *w == id)) {
                Some(at) => {
                    waiting.remove(at);
                    out.push(message);
                }
                None => out.push(json!({ "role": "user", "content": format!("Tool result:\n{}", message["content"].as_str().unwrap_or_default()) })),
            }
            continue;
        }
        close(&mut out, &mut waiting);
        if let Some(calls) = message["tool_calls"].as_array() {
            waiting = calls.iter().filter_map(|c| c["id"].as_str().map(str::to_owned)).collect();
            if message["content"].as_str().is_some_and(str::is_empty) {
                message["content"] = Value::Null;
            }
        }
        out.push(message);
    }
    close(&mut out, &mut waiting);
    out
}

/// A conversation with attachments in the OpenAI form: as `openai_conversation`, and a message
/// with images or sounds gets content parts (images as PNG `image_url`, sounds as WAV
/// `input_audio`). Files arrive as their text already.
pub fn openai_conversation_with_parts(messages: Vec<cluaiz_core::ChatMessage<Vec<cluaiz_core::media::PreparedPart>>>) -> Result<Vec<Value>, String> {
    use base64::Engine as _;
    use cluaiz_core::media::PreparedPart;
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    // A message with attachments is marked in its text, converted with the others, then given
    // its content parts
    let mut attached: Vec<Vec<Value>> = Vec::new();
    let mut pairs = Vec::with_capacity(messages.len());
    for message in messages {
        let mut text = String::new();
        let mut parts = Vec::new();
        for part in message.content {
            match part {
                PreparedPart::Text { text: part } => text.push_str(&part),
                PreparedPart::Image { image } => {
                    let png = cluaiz_media::image::encode_png(&image).map_err(|e| format!("an attached image could not be prepared: {e}"))?;
                    parts.push(json!({ "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", encode(&png)) } }));
                }
                PreparedPart::Audio { audio } => {
                    let channels = audio.channels().max(1) as usize;
                    let mono: Vec<f32> = audio.samples().chunks(channels).map(|f| f.iter().sum::<f32>() / f.len() as f32).collect();
                    let wav = cluaiz_media::wav::encode_pcm16(&mono, audio.sample_rate(), Default::default()).map_err(|e| format!("an attached sound could not be prepared: {e}"))?;
                    parts.push(json!({ "type": "input_audio", "input_audio": { "data": encode(&wav), "format": "wav" } }));
                }
            }
        }
        if !parts.is_empty() {
            text = format!("{MEDIA_MARK}{}\u{1}{text}", attached.len());
            attached.push(parts);
        }
        pairs.push((message.role, text));
    }
    let mut out = openai_conversation(&pairs);
    for message in &mut out {
        let Some(text) = message["content"].as_str().and_then(|t| t.strip_prefix(MEDIA_MARK)).map(str::to_owned) else { continue };
        let Some((index, rest)) = text.split_once('\u{1}') else { continue };
        let Some(parts) = index.parse::<usize>().ok().and_then(|i| attached.get(i)) else { continue };
        let mut content = vec![json!({ "type": "text", "text": rest })];
        content.extend(parts.iter().cloned());
        message["content"] = json!(content);
    }
    Ok(out)
}

/// Marks a message whose attachments are put back after the conversion
const MEDIA_MARK: &str = "\u{1}cluaiz-attached-";

/// A provider's model as the model of the Engine's own chat (`/v1/chat/completions` with memory,
/// the chat's ledger, Engine tools, context planning and stream controls). The Engine renders the
/// conversation as for an installed model; `dispatch` sends it to the provider and streams the
/// answer back as tokens in the Engine's form: thinking between `<think>` tags, calls as
/// `<tool_call>` blocks.
pub struct ProviderRun {
    pub target: Target,
    /// The request's own `max_tokens` (the Engine's default, the whole context, is not sent)
    pub max_tokens: Option<usize>,
    /// OpenAI fields the Engine's prompt does not carry (`stop`, `logit_bias`, `user`)
    pub extra: serde_json::Map<String, Value>,
    /// What the last generation used, in the form of a loaded model's report
    pub report: std::sync::Arc<std::sync::Mutex<Option<cluaiz_runtime::ContextReport>>>,
}

/// Sent once when a provider refused a call to a function it was not given
const TOOLS_REMINDER: &str = "Call only the functions listed in tools. To use another installed tool, call load_tool with its id first; its functions are then listed.";

/// The provider refused a call because the function was not in `tools`
fn tool_call_refused(body: &str) -> bool {
    let lower = body.to_lowercase();
    lower.contains("tool_use_failed") || lower.contains("tool call validation failed") || lower.contains("not in request.tools")
}

/// The call(s) the model made, from the refusal (`failed_generation`), in the Engine's
/// `<tool_call>` form; None when the refusal does not carry them as JSON
fn refused_calls(body: &str) -> Option<String> {
    let error: Value = serde_json::from_str(body).ok()?;
    let failed = error.pointer("/error/failed_generation")?.as_str()?.trim();
    let start = failed.find(['{', '['])?;
    let value: Value = serde_json::from_str(&failed[start..]).ok()?;
    let calls: Vec<Value> = match value {
        Value::Array(calls) => calls,
        call => vec![call],
    };
    let text: Vec<String> = calls
        .iter()
        .filter_map(|call| {
            let call = call.get("function").unwrap_or(call);
            let name = call.get("name")?.as_str()?;
            let arguments = match call.get("arguments").or_else(|| call.get("parameters")) {
                Some(Value::String(text)) => text.clone(),
                Some(arguments) => arguments.to_string(),
                None => "{}".into(),
            };
            Some(cluaiz_core::tool_calls::canonical_call(name, &arguments))
        })
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

/// The complete lines of a server-sent event stream, after `chunk` is added to `pending`. A line
/// is decoded only when whole, so a character split between two chunks arrives intact.
pub fn sse_lines(pending: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    pending.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(end) = pending.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = pending.drain(..=end).collect();
        lines.push(String::from_utf8_lossy(&line).into_owned());
    }
    lines
}

/// The thinking in a stream delta: providers name it `reasoning_content` or `reasoning`, and some
/// send both with the same text, so only the first is taken
pub fn delta_reasoning(delta: &Value) -> Option<&str> {
    ["reasoning_content", "reasoning"].iter().find_map(|field| delta[*field].as_str().filter(|t| !t.is_empty()))
}

/// The thinking tags provider answers are streamed with
pub const THINK_START: &str = "<think>";
pub const THINK_END: &str = "</think>";

impl ProviderRun {
    pub fn new(target: Target, max_tokens: Option<usize>, extra: serde_json::Map<String, Value>) -> Self {
        Self { target, max_tokens, extra, report: Default::default() }
    }

    pub fn last_report(&self) -> Option<cluaiz_runtime::ContextReport> {
        self.report.lock().ok().and_then(|report| report.clone())
    }

    /// One generation of the turn: `prompt` is the Engine's prompt (a chat envelope as JSON)
    pub async fn dispatch(&self, prompt: &str, think_mode: &str, effort: cluaiz_core::ThinkingBudget, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> cluaiz_runtime::HostResponse {
        use cluaiz_runtime::HostResponse;
        let envelope: Value = match serde_json::from_str(prompt) {
            Ok(envelope) => envelope,
            Err(error) => return HostResponse::Error(format!("the prompt for {} is not a chat envelope: {error}", self.target.full_name())),
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
        self.send(openai_conversation(&pairs), &envelope, think_mode, effort, cancel).await
    }

    /// One generation of a turn with attachments: `prepared` holds the decoded images and sounds;
    /// `prompt` (the turn's text envelope) gives the tools and the samplers
    pub async fn dispatch_prepared(&self, prepared: cluaiz_core::ChatEnvelope<Vec<cluaiz_core::media::PreparedPart>>, prompt: &str, think_mode: &str, effort: cluaiz_core::ThinkingBudget, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> cluaiz_runtime::HostResponse {
        let envelope: Value = serde_json::from_str(prompt).unwrap_or_else(|_| json!({}));
        match openai_conversation_with_parts(prepared.messages) {
            Ok(messages) => self.send(messages, &envelope, think_mode, effort, cancel).await,
            Err(error) => cluaiz_runtime::HostResponse::Error(error),
        }
    }

    async fn send(&self, messages: Vec<Value>, envelope: &Value, think_mode: &str, effort: cluaiz_core::ThinkingBudget, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> cluaiz_runtime::HostResponse {
        use cluaiz_runtime::HostResponse;
        let mut body = json!({
            "model": self.target.model,
            "messages": messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        // The request's samplers (llama names), then settings.json, as for an installed model
        let saved = cluaiz_core::Settings::current().samplers;
        let sent = &envelope["samplers"];
        let pick = |name: &str, fallback: Option<Value>| sent.get(name).filter(|v| !v.is_null()).cloned().or(fallback);
        for (field, value) in [
            ("temperature", pick("temp", saved.temp.map(|v| json!(v)))),
            ("top_p", pick("top_p", saved.top_p.map(|v| json!(v)))),
            ("presence_penalty", pick("presence_penalty", saved.presence_penalty.map(|v| json!(v)))),
            ("frequency_penalty", pick("frequency_penalty", saved.frequency_penalty.map(|v| json!(v)))),
            ("seed", pick("seed", saved.seed.map(|v| json!(v)))),
        ] {
            if let Some(value) = value {
                body[field] = value;
            }
        }
        if let Some(tools) = envelope.get("tools").filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
            body["tools"] = tools.clone();
        }
        if let Some(format) = envelope.get("response_format").filter(|v| !v.is_null()) {
            body["response_format"] = format.clone();
        }
        if let Some(max) = self.max_tokens {
            body["max_tokens"] = json!(max);
        }
        let effort = match effort {
            cluaiz_core::ThinkingBudget::Low => Some("low"),
            cluaiz_core::ThinkingBudget::Medium => Some("medium"),
            cluaiz_core::ThinkingBudget::High => Some("high"),
            _ => None,
        };
        if let (Some(effort), false) = (effort, think_mode == "off") {
            body["reasoning_effort"] = json!(effort);
        }
        for (field, value) in &self.extra {
            body[field] = value.clone();
        }

        // Sent; a request the provider says does not fit loses its oldest turns
        let target = self.target.clone();
        let mut dropped = 0usize;
        let mut turns_dropped = 0usize;
        let mut reminded = false;
        let (response, key) = loop {
            let (builder, key) = match request_for_chat(&target) {
                Ok(found) => found,
                Err(message) => return HostResponse::Error(message),
            };
            let response = match builder.header(reqwest::header::ACCEPT, "text/event-stream").body(body.to_string()).send().await {
                Ok(response) => response,
                Err(error) => return HostResponse::Error(send_error(&target, &error, key.as_deref())),
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
                    continue;
                }
            }
            // The model called a function that is not in `tools` (such as a suggested tool it did
            // not load), and the provider refused it. The Engine answers such a call itself, as
            // for an installed model: the call the provider names is passed on; without one, the
            // request is sent once more with a reminder of how tools are loaded.
            if status == 400 && tool_call_refused(&text) {
                if let Some(calls) = refused_calls(&text) {
                    let (tx, rx) = tokio::sync::mpsc::channel::<String>(1);
                    let _ = tx.try_send(calls);
                    return HostResponse::TokenStream(rx);
                }
                if !reminded {
                    reminded = true;
                    if let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) {
                        messages.push(json!({ "role": "system", "content": TOOLS_REMINDER }));
                    }
                    continue;
                }
            }
            return HostResponse::Error(message);
        };

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(512);
        let report = std::sync::Arc::clone(&self.report);
        let show_thinking = think_mode != "off";
        let n_ctx = target.context() as usize;
        tokio::spawn(async move {
            let mut response = response;
            let mut pending: Vec<u8> = Vec::new();
            let mut in_think = false;
            let mut finish = "eos";
            let mut usage: Option<(usize, usize)> = None;
            let mut calls: std::collections::BTreeMap<u64, (String, String)> = Default::default();
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            'read: loop {
                let chunk = tokio::select! {
                    chunk = response.chunk() => chunk,
                    _ = tick.tick() => {
                        if cancel.load(std::sync::atomic::Ordering::Relaxed) || tx.is_closed() {
                            finish = "cancel";
                            break;
                        }
                        continue;
                    }
                };
                let chunk = match chunk {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => break,
                    Err(error) => {
                        let _ = tx.send(format!("[ERROR: {}]", send_error(&target, &error, key.as_deref()))).await;
                        return;
                    }
                };
                for line in sse_lines(&mut pending, &chunk) {
                    let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else { continue };
                    if data == "[DONE]" {
                        break 'read;
                    }
                    let Ok(event) = serde_json::from_str::<Value>(data) else { continue };
                    if let Some(message) = event.pointer("/error/message").and_then(Value::as_str) {
                        // A refused call to a function not in `tools` goes on to the Engine, which
                        // answers it as for an installed model (see `refused_calls`)
                        if let Some(calls) = tool_call_refused(data).then(|| refused_calls(data)).flatten() {
                            pending.clear();
                            let mut text = String::new();
                            if in_think {
                                text.push_str(THINK_END);
                                in_think = false;
                            }
                            text.push('\n');
                            text.push_str(&calls);
                            let _ = tx.send(text).await;
                            break 'read;
                        }
                        let _ = tx.send(format!("[ERROR: {}: {}]", target.provider, redact(message, key.as_deref()))).await;
                        return;
                    }
                    if let Some(u) = event.get("usage").filter(|u| u.is_object()).or_else(|| event.pointer("/x_groq/usage")) {
                        let prompt = u["prompt_tokens"].as_u64().unwrap_or(0) as usize;
                        let total = u["total_tokens"].as_u64().map(|t| t as usize).unwrap_or(prompt + u["completion_tokens"].as_u64().unwrap_or(0) as usize);
                        usage = Some((prompt, total));
                    }
                    let Some(choice) = event["choices"].get(0) else { continue };
                    if choice["finish_reason"].as_str() == Some("length") {
                        finish = "limit";
                    }
                    let delta = &choice["delta"];
                    let mut out = String::new();
                    if let Some(thinking) = delta_reasoning(delta).filter(|_| show_thinking) {
                        if !in_think {
                            out.push_str(THINK_START);
                            in_think = true;
                        }
                        out.push_str(thinking);
                    }
                    if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                        if in_think {
                            out.push_str(THINK_END);
                            in_think = false;
                        }
                        out.push_str(text);
                    }
                    for (position, call) in delta["tool_calls"].as_array().into_iter().flatten().enumerate() {
                        let index = call["index"].as_u64().unwrap_or(position as u64);
                        let entry = calls.entry(index).or_default();
                        if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                            entry.0.push_str(name);
                        }
                        match call.pointer("/function/arguments") {
                            Some(Value::String(arguments)) => entry.1.push_str(arguments),
                            Some(arguments) if arguments.is_object() => entry.1.push_str(&arguments.to_string()),
                            _ => {}
                        }
                    }
                    if !out.is_empty() && tx.send(out).await.is_err() {
                        finish = "cancel";
                        break 'read;
                    }
                }
            }
            let mut tail = String::new();
            if in_think {
                tail.push_str(THINK_END);
            }
            for (name, arguments) in calls.into_values().filter(|(name, _)| !name.is_empty()) {
                let arguments = if arguments.trim().is_empty() { "{}".to_string() } else { arguments };
                tail.push('\n');
                tail.push_str(&cluaiz_core::tool_calls::canonical_call(&name, &arguments));
            }
            if !tail.is_empty() {
                let _ = tx.send(tail).await;
            }
            let (prompt_tokens, n_tokens) = usage.unwrap_or((0, 0));
            if let Ok(mut slot) = report.lock() {
                *slot = Some(cluaiz_runtime::ContextReport {
                    n_ctx,
                    prompt_tokens,
                    processed_tokens: prompt_tokens,
                    n_tokens,
                    dropped_messages: dropped,
                    truncated: finish == "limit" || dropped > 0,
                    stop: finish,
                    kv_reuse: "prefix",
                    cause: "provider",
                    ..Default::default()
                });
            }
        });
        HostResponse::TokenStream(rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_core::providers::Provider;

    fn target() -> Target {
        Target { provider: "groq".into(), config: Provider::new("https://api.groq.com/openai/v1"), model: "qwen/qwen3-32b".into() }
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
    fn a_streamed_wav_gets_its_real_sizes() {
        let mut wav = b"RIFF\xff\xff\xff\xffWAVEfmt \x10\x00\x00\x00".to_vec();
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"LIST\x02\x00\x00\x00ab");
        wav.extend_from_slice(b"data\xff\xff\xff\xff");
        wav.extend_from_slice(&[1u8, 2, 3, 4]);
        let fixed = wav_with_sizes(&wav);
        assert_eq!(u32::from_le_bytes(fixed[4..8].try_into().unwrap()) as usize, fixed.len() - 8);
        let data = fixed.len() - 12;
        assert_eq!(&fixed[data..data + 4], b"data");
        assert_eq!(u32::from_le_bytes(fixed[data + 4..data + 8].try_into().unwrap()), 4);
        assert_eq!(wav_with_sizes(b"ID3 not a wav"), b"ID3 not a wav".to_vec());
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

    #[test]
    fn a_character_split_between_chunks_arrives_whole() {
        let line = "data: {\"t\":\"नमस्ते 👋\"}\n".as_bytes();
        let cut = line.iter().position(|&b| b >= 0x80).unwrap() + 1;
        let mut pending = Vec::new();
        assert!(sse_lines(&mut pending, &line[..cut]).is_empty());
        assert_eq!(sse_lines(&mut pending, &line[cut..]), vec!["data: {\"t\":\"नमस्ते 👋\"}\n".to_string()]);
        assert!(pending.is_empty());
        assert_eq!(delta_reasoning(&json!({ "reasoning_content": "a", "reasoning": "a" })), Some("a"));
        assert_eq!(delta_reasoning(&json!({ "reasoning_content": "", "reasoning": "b" })), Some("b"));
    }

    #[test]
    fn a_refused_call_is_passed_on_to_the_engine() {
        let body = r#"{"error":{"message":"tool call validation failed: attempted to call tool 'math_evaluate' which was not in request.tools","type":"invalid_request_error","code":"tool_use_failed","failed_generation":"{\"name\": \"math_evaluate\", \"arguments\": {\"expression\": \"2**40\"}}"}}"#;
        assert!(tool_call_refused(body));
        let calls = refused_calls(body).unwrap();
        let (_, found) = cluaiz_core::tool_calls::split_calls(&calls);
        assert_eq!(found, vec![("math_evaluate".to_string(), "{\"expression\":\"2**40\"}".to_string())]);
        assert!(refused_calls(r#"{"error":{"message":"tool call validation failed","failed_generation":"I will call it"}}"#).is_none());
        assert!(!tool_call_refused(r#"{"error":{"message":"Invalid API Key"}}"#));
    }
}
