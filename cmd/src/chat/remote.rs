//! A running `cluaiz serve` as the model: the CLI sends its turns to `/v1/chat/completions`
//! instead of loading the model a second time (two copies split the VRAM and shrink the context).
//! Tools are sent as OpenAI function definitions, so the server returns their calls unexecuted and
//! the CLI runs them, asking first, as it does with a local model.

use std::sync::atomic::Ordering;
use std::time::Duration;

use cluaiz_core::{media::PreparedPart, ChatMessage};
use cluaiz_plan::ExecutionPlan;
use color_eyre::{eyre::eyre, Result};
use serde_json::{json, Value};

use super::{output, ChatOptions, TurnReport, GENERATING};
use crate::cli::server;

pub struct Remote {
    pub url: String,
}

impl Remote {
    /// The server's plan for `model_id` after it loaded it, or None when no server answers
    pub async fn attach(model_id: &str) -> Option<(Self, ExecutionPlan)> {
        // A quick look first: no server must not cost the user a long wait
        let alive = server::request_with_timeout(reqwest::Method::GET, "/v1/models", Duration::from_millis(800))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if !alive {
            return None;
        }
        let _spin = crate::ui::spinner::Spinner::start("Loading the model in the running server…");
        server::cep("models.load", json!({ "model": model_id })).await.ok()?;
        let ps = server::cep("system.ps", json!({})).await.ok()?;
        let plan = ps["active_processes"]
            .as_array()?
            .iter()
            .find(|process| process["model_id"] == model_id)
            .and_then(|process| serde_json::from_value(process["plan"].clone()).ok())?;
        Some((Self { url: server::base_url() }, plan))
    }

    /// One answer, streamed into `out`; a tool call comes back in the Engine's `<tool_call>` form
    pub async fn generate(
        &self,
        model_id: &str,
        options: &ChatOptions,
        messages: Vec<ChatMessage<Vec<PreparedPart>>>,
        tools: Option<&Value>,
        out: &mut dyn output::Output,
    ) -> Result<(String, TurnReport)> {
        let mut sent = Vec::with_capacity(messages.len());
        for message in messages {
            let mut text = String::new();
            for part in message.content {
                match part {
                    PreparedPart::Text { text: part } => text.push_str(&part),
                    _ => return Err(eyre!("attachments need the model in this process: start with --local")),
                }
            }
            sent.push(json!({ "role": message.role, "content": text }));
        }
        let s = &options.samplers;
        let mut body = json!({
            "model": model_id,
            "messages": sent,
            "stream": true,
            "tool_search": false,
            "temperature": s.temp, "top_p": s.top_p, "top_k": s.top_k, "min_p": s.min_p,
            "presence_penalty": s.presence_penalty, "frequency_penalty": s.frequency_penalty,
            "repetition_penalty": s.repeat_penalty, "seed": s.seed,
            "max_tokens": options.max_tokens,
        });
        if let Some(think) = options.think {
            body["think_mode"] = json!(think.name());
        }
        if let Some(effort) = options.effort {
            body["reasoning_effort"] = serde_json::to_value(effort).unwrap_or(Value::Null);
        }
        if let Some(tools) = tools.filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
            body["tools"] = tools.clone();
        }

        cluaiz_core::live::start();
        GENERATING.store(true, Ordering::SeqCst);
        let result = self.stream(body, out).await;
        GENERATING.store(false, Ordering::SeqCst);
        let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.swap(false, Ordering::SeqCst);
        out.finish();
        let (answer, finish, context) = result?;
        let stop = match finish.as_str() {
            "length" => "limit",
            _ if cancelled => "cancel",
            _ => "eos",
        };
        let used = context.as_ref().and_then(|plan| Some((plan["used_tokens"].as_u64()? as usize + plan["protected_tokens"].as_u64().unwrap_or(0) as usize, plan["n_ctx"].as_u64()? as usize)));
        let dropped = context.as_ref().and_then(|plan| plan["dropped_messages"].as_u64()).unwrap_or(0) as usize;
        Ok((answer, TurnReport { timings: None, thinking: None, cancelled, stop, context: used, dropped }))
    }

    /// The answer, why it ended, and the server's context plan (`context_report.plan`) when sent
    async fn stream(&self, body: Value, out: &mut dyn output::Output) -> Result<(String, String, Option<Value>)> {
        let mut response = server::request_with_timeout(reqwest::Method::POST, "/v1/chat/completions", Duration::from_secs(24 * 3600))
            .json(&body)
            .send()
            .await
            .map_err(|e| eyre!("the server at {} did not answer: {e}", self.url))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
                .unwrap_or(text);
            return Err(eyre!("server: {status}: {message}"));
        }
        let mut answer = String::new();
        let mut finish = String::from("stop");
        let mut plan = None;
        let mut pending = String::new();
        let mut tick = tokio::time::interval(Duration::from_millis(120));
        loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => chunk.map_err(|e| eyre!("server stream: {e}"))?,
                _ = tick.tick() => {
                    out.tick();
                    // Ctrl+C: dropping the stream ends the request on the server
                    if cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) {
                        break;
                    }
                    continue;
                }
            };
            let Some(chunk) = chunk else { break };
            pending.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(end) = pending.find('\n') {
                let line: String = pending.drain(..=end).collect();
                let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else { continue };
                if data == "[DONE]" {
                    return Ok((answer, finish, plan));
                }
                let Ok(event) = serde_json::from_str::<Value>(data) else { continue };
                if event["context_report"]["plan"].is_object() {
                    plan = Some(event["context_report"]["plan"].clone());
                }
                if let Some(message) = event["error"]["message"].as_str() {
                    return Err(eyre!("{message}"));
                }
                let choice = &event["choices"][0];
                if let Some(reason) = choice["finish_reason"].as_str() {
                    finish = reason.to_owned();
                }
                let delta = &choice["delta"];
                if let Some(thinking) = delta["reasoning_content"].as_str() {
                    out.reasoning(thinking);
                }
                if let Some(text) = delta["content"].as_str() {
                    out.content(text);
                    answer.push_str(text);
                }
                for call in delta["tool_calls"].as_array().into_iter().flatten() {
                    let name = call["function"]["name"].as_str().unwrap_or_default();
                    let arguments = call["function"]["arguments"].as_str().unwrap_or("{}");
                    let block = format!("\n{}", cluaiz_core::tool_calls::canonical_call(name, arguments));
                    cluaiz_core::live::set_call(name, arguments);
                    out.content(&block);
                    answer.push_str(&block);
                }
            }
        }
        Ok((answer, finish, plan))
    }
}
