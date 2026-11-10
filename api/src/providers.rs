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
            format!("the request exceeds the available context of {name} ({status})")
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
/// providers use (`context_length` at OpenRouter, `context_window` at Groq, and others).
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

/// One short chat request: the key, the URL and the model work. Returns the model's answer.
pub async fn test(target: &Target) -> Result<String, String> {
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

/// `/v1/chat/completions` for a provider's model: the request goes to the provider with its OpenAI
/// fields, and its answer (a stream or JSON) comes back as the provider sent it. The Engine's own
/// features (memory, server-side tools, context control) do not apply to it.
pub async fn forward(target: Target, request: Value) -> axum::response::Response {
    use axum::response::IntoResponse;
    let fail = |status: u16, message: String| {
        let status = axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
        (status, axum::Json(json!({ "error": { "message": message, "type": "provider_error", "provider": target.provider } }))).into_response()
    };
    let body = chat_body(&target, &request);
    let streaming = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    let (builder, key) = match request_for_chat(&target) {
        Ok(found) => found,
        Err(message) => return fail(400, message),
    };
    let response = match builder.body(body.to_string()).send().await {
        Ok(response) => response,
        Err(error) => return fail(502, send_error(&target, &error, key.as_deref())),
    };
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let text = response.text().await.unwrap_or_default();
        return fail(status, error_message(&target, status, &text, key.as_deref()));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or(if streaming { "text/event-stream" } else { "application/json" })
        .to_string();
    let mut response = response;
    let stream = async_stream::stream! {
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => yield Ok::<_, std::io::Error>(chunk),
                Ok(None) => break,
                Err(error) => {
                    tracing::warn!("provider stream ended early: {}", redact(&error.to_string(), key.as_deref()));
                    break;
                }
            }
        }
    };
    axum::response::Response::builder()
        .status(status)
        .header(axum::http::header::CONTENT_TYPE, content_type)
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .body(axum::body::Body::from_stream(stream))
        .unwrap_or_else(|_| fail(500, "the provider's answer could not be passed on".into()))
}

fn request_for_chat(target: &Target) -> Result<(reqwest::RequestBuilder, Option<String>), String> {
    let (builder, key) = request(&target.config, reqwest::Method::POST, &target.config.chat_url())?;
    Ok((builder.timeout(Duration::from_secs(24 * 3600)).header(reqwest::header::CONTENT_TYPE, "application/json"), key))
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
}
