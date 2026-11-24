//! Ollama's chat, generate, embed and version routes on top of the Engine's own handlers.
//!
//! An Ollama request is turned into the OpenAI request the chat and embeddings handlers already
//! take, and their answer is turned back into Ollama's shape: one JSON object, or, when streaming
//! (Ollama's default), one JSON object per line (`application/x-ndjson`). Nothing is generated here;
//! every token comes from the same chat pipeline as `/v1/chat/completions`.

use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::StreamExt;
use serde_json::{json, Map, Value};

use crate::AppState;

/// `POST /api/chat`: the client's messages and the client's own tools, with nothing the Engine
/// would add to a chat (`plain_prompt`)
pub async fn chat(state: State<Arc<AppState>>, body: Bytes) -> Response {
    run(state, body, Kind::Chat).await
}

/// `POST /api/generate`: a prompt (and an optional system text) as one chat turn, with nothing
/// the Engine would add to a chat (`plain_prompt`)
pub async fn generate(state: State<Arc<AppState>>, body: Bytes) -> Response {
    run(state, body, Kind::Generate).await
}

/// `GET /api/version`: this Engine's version
pub async fn version() -> Json<Value> {
    Json(json!({ "version": env!("CARGO_PKG_VERSION") }))
}

/// `GET /api/ps`: the chat model this server has loaded, in Ollama's shape. `size_vram` is the VRAM the
/// plan gave the model (planned, not read from the driver); `size` is its files on disk
pub async fn ps(State(state): State<Arc<AppState>>) -> Json<Value> {
    let plan = state.host.current_plan.lock().await.clone();
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let models: Vec<Value> = plan
        .into_iter()
        .map(|plan| {
            let entry = registry.installed_models.get(&plan.model_id);
            json!({
                "name": plan.model_id,
                "model": plan.model_id,
                "size": entry.map(|e| e.size_bytes()).unwrap_or(0),
                "size_vram": (plan.vram_budget_gb * 1e9) as u64,
                "context_length": plan.context_window,
                "details": {
                    "format": entry.map(|e| e.format_type.clone()),
                    "family": entry.map(|e| e.metadata.architecture.clone()),
                    "parameter_size": entry.map(super::models::parameter_size),
                },
            })
        })
        .collect();
    Json(json!({ "models": models }))
}

/// `POST /api/embed`: `{"model", "input": text or list}` → `{"model", "embeddings": [[...]]}`
pub async fn embed(state: State<Arc<AppState>>, body: Bytes) -> Response {
    let request: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return error(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };
    let mut openai = Map::new();
    if let Some(model) = request.get("model") {
        openai.insert("model".into(), model.clone());
    }
    openai.insert("input".into(), request.get("input").cloned().unwrap_or(Value::Null));
    if let Some(minutes) = request.get("keep_alive").and_then(keep_alive_minutes) {
        openai.insert("keep_alive".into(), json!(minutes));
    }
    let inner = super::embeddings::embeddings_entry(state, Bytes::from(Value::Object(openai).to_string())).await;
    let status = inner.status();
    let bytes = axum::body::to_bytes(inner.into_body(), usize::MAX).await.unwrap_or_default();
    let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() {
        return error(status, &error_text(&reply, &bytes));
    }
    let embeddings: Vec<Value> = reply["data"]
        .as_array()
        .map(|items| items.iter().map(|item| item["embedding"].clone()).collect())
        .unwrap_or_default();
    Json(json!({
        "model": reply.get("model").cloned().or_else(|| request.get("model").cloned()).unwrap_or(Value::Null),
        "embeddings": embeddings,
        "prompt_eval_count": reply["usage"]["prompt_tokens"],
    }))
    .into_response()
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Chat,
    Generate,
}

async fn run(state: State<Arc<AppState>>, body: Bytes, kind: Kind) -> Response {
    let request: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return error(StatusCode::BAD_REQUEST, &format!("invalid JSON: {e}")),
    };
    let stream = request.get("stream").and_then(Value::as_bool).unwrap_or(true);
    let model = request.get("model").cloned().unwrap_or(Value::Null);
    let openai = to_openai(&request, kind, stream);
    let mut chat_request: super::chat::types::ExternalChatRequest = match serde_json::from_value(openai) {
        Ok(r) => r,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e.to_string()),
    };
    // Ollama renders only what the client sent with the model's template: generate the system
    // text and the prompt with no tools, chat the messages with the request's `tools`
    // (`GenerateHandler` and `ChatHandler` in ollama/server/routes.go). The Engine adds nothing
    // either; the client's tools still go to the model (`to_openai`)
    chat_request.plain_prompt = true;
    let inner = super::chat::chat_completions(state, Json(chat_request)).await;
    let status = inner.status();
    if !status.is_success() {
        let bytes = axum::body::to_bytes(inner.into_body(), usize::MAX).await.unwrap_or_default();
        let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        return error(status, &error_text(&reply, &bytes));
    }
    if !stream {
        let bytes = axum::body::to_bytes(inner.into_body(), usize::MAX).await.unwrap_or_default();
        let reply: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if reply.get("error").is_some() {
            return error(StatusCode::INTERNAL_SERVER_ERROR, &error_text(&reply, &bytes));
        }
        return Json(final_object(&reply, &model, kind)).into_response();
    }
    ndjson(inner, model, kind)
}

/// The OpenAI request for an Ollama chat or generate request
fn to_openai(request: &Value, kind: Kind, stream: bool) -> Value {
    let mut out = Map::new();
    if let Some(model) = request.get("model") {
        out.insert("model".into(), model.clone());
    }
    let messages = match kind {
        Kind::Chat => request["messages"].as_array().map(|m| m.iter().map(message).collect()).unwrap_or_default(),
        Kind::Generate => {
            let mut m = Vec::new();
            if let Some(system) = request.get("system").and_then(Value::as_str) {
                m.push(json!({"role": "system", "content": system}));
            }
            let mut user = json!({"role": "user", "content": request["prompt"].as_str().unwrap_or("")});
            if let Some(images) = request.get("images") {
                user["images"] = images.clone();
            }
            m.push(message(&user));
            m
        }
    };
    out.insert("messages".into(), Value::Array(messages));
    out.insert("stream".into(), json!(stream));
    if stream {
        out.insert("stream_options".into(), json!({"include_usage": true}));
    }
    // Ollama's generate request has no `tools` field; only chat takes them
    if let Some(tools) = request.get("tools").filter(|_| kind == Kind::Chat) {
        out.insert("tools".into(), tools.clone());
    }
    // Ollama's `think`: true / false, or a level for models that take one
    match request.get("think") {
        Some(Value::Bool(on)) => {
            out.insert("think_mode".into(), json!(if *on { "on" } else { "off" }));
        }
        Some(Value::String(level)) => {
            out.insert("think_mode".into(), json!("on"));
            out.insert("reasoning_effort".into(), json!(level));
        }
        _ => {}
    }
    // `format`: "json" or a JSON schema
    match request.get("format") {
        Some(Value::String(f)) if f == "json" => {
            out.insert("response_format".into(), json!({"type": "json_object"}));
        }
        Some(schema @ Value::Object(_)) => {
            out.insert("response_format".into(), json!({"type": "json_schema", "json_schema": {"name": "schema", "schema": schema}}));
        }
        _ => {}
    }
    if let Some(minutes) = request.get("keep_alive").and_then(keep_alive_minutes) {
        out.insert("keep_alive".into(), json!(minutes));
    }
    if let Some(options) = request.get("options").and_then(Value::as_object) {
        for (from, to) in [
            ("temperature", "temperature"),
            ("top_p", "top_p"),
            ("top_k", "top_k"),
            ("min_p", "min_p"),
            ("seed", "seed"),
            ("presence_penalty", "presence_penalty"),
            ("frequency_penalty", "frequency_penalty"),
            ("repeat_penalty", "repetition_penalty"),
            ("num_predict", "max_tokens"),
            ("stop", "stop"),
        ] {
            if let Some(v) = options.get(from) {
                // num_predict -1 means unlimited in Ollama: no limit is sent
                if from == "num_predict" && v.as_i64().is_some_and(|n| n < 0) {
                    continue;
                }
                out.insert(to.into(), v.clone());
            }
        }
        if let Some(n_ctx) = options.get("num_ctx") {
            out.insert("llama".into(), json!({"n_ctx": n_ctx}));
        }
    }
    Value::Object(out)
}

/// One Ollama message as an OpenAI message: `images` (base64) become image parts, tool-call
/// arguments (an object in Ollama) become the JSON text OpenAI uses
fn message(m: &Value) -> Value {
    let role = m["role"].as_str().unwrap_or("user");
    let text = m["content"].as_str().unwrap_or("");
    let mut out = json!({"role": role});
    match m.get("images").and_then(Value::as_array).filter(|i| !i.is_empty()) {
        Some(images) => {
            let mut parts = vec![json!({"type": "text", "text": text})];
            for image in images.iter().filter_map(Value::as_str) {
                let url = if image.starts_with("data:") { image.to_string() } else { format!("data:image/png;base64,{image}") };
                parts.push(json!({"type": "image_url", "image_url": {"url": url}}));
            }
            out["content"] = Value::Array(parts);
        }
        None => out["content"] = json!(text),
    }
    if let Some(calls) = m.get("tool_calls").and_then(Value::as_array) {
        let calls: Vec<Value> = calls
            .iter()
            .enumerate()
            .map(|(i, call)| {
                let f = &call["function"];
                let args = match &f["arguments"] {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                json!({"id": format!("call_{i}"), "type": "function", "function": {"name": f["name"], "arguments": args}})
            })
            .collect();
        out["tool_calls"] = Value::Array(calls);
    }
    if let Some(name) = m.get("tool_name").or_else(|| m.get("name")) {
        out["name"] = name.clone();
    }
    out
}

/// Ollama's `keep_alive`: a number is seconds, a string is a duration ("5m"); the Engine takes minutes
fn keep_alive_minutes(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_f64().map(|s| if s < 0.0 { -1 } else { (s / 60.0).ceil() as i64 }),
        Value::String(s) => {
            let s = s.trim();
            if let Ok(seconds) = s.parse::<f64>() {
                return Some(if seconds < 0.0 { -1 } else { (seconds / 60.0).ceil() as i64 });
            }
            let at = s.find(|c: char| c.is_ascii_alphabetic())?;
            let n: f64 = s[..at].trim().parse().ok()?;
            if n < 0.0 {
                return Some(-1);
            }
            let seconds = match &s[at..] {
                "ms" => n / 1000.0,
                "s" => n,
                "m" => n * 60.0,
                "h" => n * 3600.0,
                _ => return None,
            };
            Some((seconds / 60.0).ceil() as i64)
        }
        _ => None,
    }
}

/// Ollama's `tool_calls`: the arguments as an object
fn tool_calls(calls: &Value) -> Option<Value> {
    let calls = calls.as_array().filter(|c| !c.is_empty())?;
    Some(Value::Array(
        calls
            .iter()
            .map(|call| {
                let f = &call["function"];
                let args = f["arguments"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_else(|| f["arguments"].clone());
                json!({"function": {"name": f["name"], "arguments": args}})
            })
            .collect(),
    ))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The closing object (and the whole answer when not streaming)
fn final_object(reply: &Value, model: &Value, kind: Kind) -> Value {
    let choice = &reply["choices"][0];
    let msg = &choice["message"];
    let content = msg["content"].as_str().unwrap_or("");
    let mut out = json!({ "model": reply.get("model").cloned().unwrap_or_else(|| model.clone()), "created_at": now() });
    add_answer(&mut out, kind, content, msg["reasoning_content"].as_str(), tool_calls(&msg["tool_calls"]));
    out["done"] = json!(true);
    out["done_reason"] = json!(choice["finish_reason"].as_str().unwrap_or("stop"));
    add_usage(&mut out, &reply["usage"]);
    out
}

fn add_answer(out: &mut Value, kind: Kind, content: &str, thinking: Option<&str>, calls: Option<Value>) {
    match kind {
        Kind::Chat => {
            let mut message = json!({"role": "assistant", "content": content});
            if let Some(t) = thinking.filter(|t| !t.is_empty()) {
                message["thinking"] = json!(t);
            }
            if let Some(c) = calls {
                message["tool_calls"] = c;
            }
            out["message"] = message;
        }
        Kind::Generate => {
            out["response"] = json!(content);
            if let Some(t) = thinking.filter(|t| !t.is_empty()) {
                out["thinking"] = json!(t);
            }
        }
    }
}

fn add_usage(out: &mut Value, usage: &Value) {
    if let Some(n) = usage["prompt_tokens"].as_u64() {
        out["prompt_eval_count"] = json!(n);
    }
    if let Some(n) = usage["completion_tokens"].as_u64() {
        out["eval_count"] = json!(n);
    }
}

/// The OpenAI Server-Sent Events of the chat handler as Ollama's lines of JSON
fn ndjson(inner: Response, model: Value, kind: Kind) -> Response {
    let mut body = inner.into_body().into_data_stream();
    let lines = async_stream::stream! {
        let mut buffer = String::new();
        let mut finish = String::from("stop");
        let mut usage = Value::Null;
        let mut ended = false;
        while let Some(chunk) = body.next().await {
            let Ok(chunk) = chunk else { break };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(at) = buffer.find("\n\n") {
                let event: String = buffer.drain(..at + 2).collect();
                for line in event.lines() {
                    let Some(data) = line.strip_prefix("data:") else { continue };
                    let data = data.trim();
                    if data == "[DONE]" {
                        ended = true;
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(data) else { continue };
                    if let Some(err) = value.get("error") {
                        let text = err["message"].as_str().unwrap_or("error").to_string();
                        yield Ok::<_, std::io::Error>(Bytes::from(format!("{}\n", json!({"error": text}))));
                        continue;
                    }
                    if value["usage"]["completion_tokens"].is_u64() {
                        usage = value["usage"].clone();
                    }
                    for choice in value["choices"].as_array().into_iter().flatten() {
                        let delta = &choice["delta"];
                        let content = delta["content"].as_str().unwrap_or("");
                        let thinking = delta["reasoning_content"].as_str();
                        let calls = tool_calls(&delta["tool_calls"]);
                        if !content.is_empty() || thinking.is_some_and(|t| !t.is_empty()) || calls.is_some() {
                            let mut out = json!({"model": model, "created_at": now()});
                            add_answer(&mut out, kind, content, thinking, calls);
                            out["done"] = json!(false);
                            yield Ok(Bytes::from(format!("{out}\n")));
                        }
                        if let Some(reason) = choice["finish_reason"].as_str() {
                            finish = reason.to_string();
                        }
                    }
                }
            }
        }
        let mut last = json!({"model": model, "created_at": now()});
        add_answer(&mut last, kind, "", None, None);
        last["done"] = json!(true);
        last["done_reason"] = json!(if ended { finish } else { "stop".to_string() });
        add_usage(&mut last, &usage);
        yield Ok(Bytes::from(format!("{last}\n")));
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(lines))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn error_text(reply: &Value, raw: &[u8]) -> String {
    reply["error"]["message"]
        .as_str()
        .or_else(|| reply["error"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| String::from_utf8_lossy(raw).trim().to_string())
}

/// Ollama's error body: `{"error": "..."}`
fn error(status: StatusCode, text: &str) -> Response {
    (status, Json(json!({ "error": text }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ollama_chat_becomes_the_openai_request() {
        let request = json!({
            "model": "m", "messages": [{"role": "user", "content": "hi", "images": ["QUJD"]}],
            "think": false, "format": "json", "keep_alive": "5m",
            "options": {"temperature": 0.2, "num_predict": 64, "num_ctx": 4096, "repeat_penalty": 1.1, "stop": ["x"]}
        });
        let out = to_openai(&request, Kind::Chat, true);
        assert_eq!(out["think_mode"], "off");
        assert_eq!(out["response_format"]["type"], "json_object");
        assert_eq!(out["keep_alive"], 5);
        assert_eq!(out["max_tokens"], 64);
        assert_eq!(out["llama"]["n_ctx"], 4096);
        assert_eq!(out["repetition_penalty"], 1.1);
        assert_eq!(out["stop"], json!(["x"]));
        assert_eq!(out["stream_options"]["include_usage"], true);
        assert_eq!(out["messages"][0]["content"][1]["image_url"]["url"], "data:image/png;base64,QUJD");
    }

    #[test]
    fn keep_alive_numbers_are_seconds_in_ollama() {
        assert_eq!(keep_alive_minutes(&json!(300)), Some(5));
        assert_eq!(keep_alive_minutes(&json!(0)), Some(0));
        assert_eq!(keep_alive_minutes(&json!(-1)), Some(-1));
        assert_eq!(keep_alive_minutes(&json!("10m")), Some(10));
        assert_eq!(keep_alive_minutes(&json!("90s")), Some(2));
    }

    #[test]
    fn a_generate_prompt_is_one_user_turn_after_the_system_text() {
        let out = to_openai(&json!({"model": "m", "prompt": "p", "system": "s"}), Kind::Generate, false);
        assert_eq!(out["messages"][0], json!({"role": "system", "content": "s"}));
        assert_eq!(out["messages"][1], json!({"role": "user", "content": "p"}));
        // Ollama's generate has no tools; chat passes the client's on
        let tools = json!([{"type": "function", "function": {"name": "f"}}]);
        assert!(to_openai(&json!({"model": "m", "prompt": "p", "tools": tools}), Kind::Generate, false).get("tools").is_none());
        assert_eq!(to_openai(&json!({"model": "m", "messages": [], "tools": tools}), Kind::Chat, false)["tools"], tools);
        assert!(out.get("stream_options").is_none());
    }

    #[test]
    fn the_openai_reply_becomes_ollamas_final_object() {
        let reply = json!({"model": "m", "choices": [{"message": {"content": "hello", "reasoning_content": "t",
            "tool_calls": [{"function": {"name": "f", "arguments": "{\"a\":1}"}}]}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 7, "completion_tokens": 2}});
        let out = final_object(&reply, &json!("m"), Kind::Chat);
        assert_eq!(out["message"]["content"], "hello");
        assert_eq!(out["message"]["thinking"], "t");
        assert_eq!(out["message"]["tool_calls"][0]["function"]["arguments"]["a"], 1);
        assert_eq!(out["done"], true);
        assert_eq!(out["prompt_eval_count"], 7);
        assert_eq!(out["eval_count"], 2);
        let generated = final_object(&reply, &json!("m"), Kind::Generate);
        assert_eq!(generated["response"], "hello");
    }
}
