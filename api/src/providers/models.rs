//! The models a provider offers (`GET {base_url}/models`) and one short test request of a
//! model's kind (`cluaiz provider test`, `providers.test`).

use std::time::Duration;

use cluaiz_core::providers::{Provider, Target};
use serde_json::{json, Value};

use super::errors::{error_message, send_error};
use super::http::request;

/// A model a provider offers, with the context size it reports
#[derive(Debug, Clone, PartialEq)]
pub struct OfferedModel {
    pub id: String,
    pub context: Option<u32>,
}

/// `GET {base_url}/models`: the models the provider offers. Context sizes come from the fields
/// providers use (`context_length`, `context_window` and others).
pub async fn models(name: &str, provider: &Provider) -> Result<Vec<OfferedModel>, String> {
    let target = Target {
        provider: name.to_string(),
        config: provider.clone(),
        model: String::new(),
    };
    let (builder, key) = request(provider, reqwest::Method::GET, &provider.models_url()).await?;
    let response = super::http::send(builder.timeout(Duration::from_secs(30)), provider)
        .await
        .map_err(|e| send_error(&target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(&target, status, &text, key.as_deref()));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{name}: the model list is not JSON: {e}"))?;
    Ok(parse_models(&value))
}

fn parse_models(value: &Value) -> Vec<OfferedModel> {
    let list = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut models: Vec<OfferedModel> = list
        .iter()
        .filter_map(|m| {
            let id = m
                .get("id")
                .or_else(|| m.get("name"))
                .and_then(Value::as_str)?
                .to_string();
            let context = [
                "context_length",
                "context_window",
                "max_input_tokens",
                "max_context_length",
                "max_model_len",
                "context_size",
            ]
            .iter()
            .find_map(|field| m.get(*field).and_then(Value::as_u64))
            .or_else(|| {
                m.pointer("/top_provider/context_length")
                    .and_then(Value::as_u64)
            })
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
        Some("embedding") => {
            let size = |body: &[u8]| {
                let size = serde_json::from_slice::<Value>(body)
                    .ok()?
                    .pointer("/data/0/embedding")?
                    .as_array()?
                    .len();
                Some(format!("a vector of {size} numbers"))
            };
            let body = json!({ "model": target.model, "input": "ready" });
            match test_other(target, "/embeddings", body.clone(), size).await {
                // Query/document models (NVIDIA, Voyage) need to be told which one the text is
                Err(e) if e.contains("input_type") => {
                    let mut body = body;
                    body["input_type"] = json!("query");
                    test_other(target, "/embeddings", body, size)
                        .await
                        .map(|answer| format!("{answer} (this model needs \"input_type\": \"query\" or \"passage\")"))
                }
                other => other,
            }
        }
        Some("tts") => {
            let mut body =
                json!({ "model": target.model, "input": "ready", "response_format": "wav" });
            if let Some(voice) = voice {
                body["voice"] = json!(voice);
            }
            test_other(target, "/audio/speech", body, |body| {
                Some(format!("{} bytes of audio", body.len()))
            })
            .await
        }
        Some("stt") => Err(format!(
            "{} is a speech-to-text model: test it by sending audio to /v1/audio/transcriptions",
            target.full_name()
        )),
        _ => test_chat(target).await,
    }
}

async fn test_other(
    target: &Target,
    path: &str,
    body: Value,
    describe: impl Fn(&[u8]) -> Option<String>,
) -> Result<String, String> {
    let url = format!("{}{path}", target.config.base_url.trim_end_matches('/'));
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url).await?;
    let response = super::http::send(
        builder
            .timeout(Duration::from_secs(60))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string()),
        &target.config,
    )
    .await
    .map_err(|e| send_error(target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let bytes = response.bytes().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(
            target,
            status,
            &String::from_utf8_lossy(&bytes),
            key.as_deref(),
        ));
    }
    describe(&bytes)
        .ok_or_else(|| format!("{}: the answer is not what {path} returns", target.provider))
}

async fn test_chat(target: &Target) -> Result<String, String> {
    let compat = super::compat::Compat::of(target);
    let mut body = json!({
        "model": target.model,
        "messages": [{ "role": "user", "content": "Reply with the single word: ready" }],
        "stream": false,
    });
    if compat.wire == super::compat::Wire::Anthropic {
        // Claude: `max_tokens` is required; on Vertex AI the model is in the URL
        body["max_tokens"] = json!(64);
        if compat.vertex {
            body.as_object_mut().map(|b| b.remove("model"));
            body["anthropic_version"] = json!(super::anthropic::VERTEX_VERSION);
        }
    } else {
        body[compat.max_tokens_field] = json!(64);
    }
    let url = super::compat::chat_url(target, false);
    let (builder, key) = request(&target.config, reqwest::Method::POST, &url).await?;
    let response = super::http::send(
        builder
            .timeout(Duration::from_secs(60))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string()),
        &target.config,
    )
    .await
    .map_err(|e| send_error(target, &e, key.as_deref()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(error_message(target, status, &text, key.as_deref()));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{}: the answer is not JSON: {e}", target.provider))?;
    // Claude answers with content blocks
    if let Some(blocks) = value["content"].as_array() {
        let text: String = blocks.iter().filter_map(|b| b["text"].as_str()).collect();
        // A model that spent the few test tokens thinking answers with its thinking
        let text = if text.trim().is_empty() { blocks.iter().filter_map(|b| b["thinking"].as_str()).collect() } else { text };
        return Ok(text.trim().chars().take(200).collect());
    }
    let message = &value["choices"][0]["message"];
    let answer = message["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| message["reasoning_content"].as_str())
        .or_else(|| message["reasoning"].as_str())
        .unwrap_or_default();
    Ok(answer.trim().chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_sizes_are_read_from_each_providers_field() {
        let models = parse_models(&json!({ "data": [
            { "id": "b", "context_window": 131072 },
            { "id": "a", "context_length": 8192 },
            { "id": "c", "top_provider": { "context_length": 4096 } },
            { "id": "d" },
        ]}));
        let ids: Vec<_> = models.iter().map(|m| (m.id.as_str(), m.context)).collect();
        assert_eq!(
            ids,
            vec![
                ("a", Some(8192)),
                ("b", Some(131072)),
                ("c", Some(4096)),
                ("d", None)
            ]
        );
    }
}
