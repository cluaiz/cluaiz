//! A provider's model (`config/providers.json`) as the model of `cluaiz run` and `cluaiz agent`.
//! The conversation is sent in the OpenAI Chat Completions format: calls in the Engine's
//! `<tool_call>` form become `tool_calls`, each tool result answers its call by `tool_call_id`, and
//! the calls the provider returns come back in the `<tool_call>` form the agent reads.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use cluaiz_core::{media::PreparedPart, providers::Target, ChatMessage};
use cluaiz_models::StreamingReasoningFilter;
use color_eyre::{eyre::eyre, Result};
use serde_json::{json, Value};

use super::{output, ChatOptions, TurnReport, GENERATING};

pub struct ProviderChat {
    pub target: Target,
    /// What this run's flags do with a provider's model (shown once, at the start)
    pub notes: Vec<String>,
}

/// The provider's model with this run's flags: `-c N` keeps the conversation within N tokens (the
/// provider's own limit still applies); load flags have no meaning there and are named in a note,
/// as are thinking flags a provider cannot take.
pub fn prepare(mut target: Target, options: &ChatOptions) -> ProviderChat {
    let mut notes = Vec::new();
    let load = &options.load;
    if let Some(n) = load.ctx_size.filter(|n| *n > 0) {
        let n = n.min(u32::MAX as i64) as u32;
        let known = target.config.models.get(&target.model).and_then(|m| m.context);
        let used = known.map_or(n, |limit| n.min(limit));
        if known.is_some_and(|limit| n > limit) {
            notes.push(format!("-c {n}: {} takes at most {used} tokens; using {used}", target.full_name()));
        } else {
            notes.push(format!("-c {n}: the conversation is kept within {n} tokens"));
        }
        target.config.models.entry(target.model.clone()).or_default().context = Some(used);
    }
    let flags: [(&str, bool); 14] = [
        ("--n-gpu-layers", load.n_gpu_layers.is_some()),
        ("--n-cpu-moe", load.n_cpu_moe.is_some()),
        ("--cache-type-k", load.cache_type_k.is_some()),
        ("--cache-type-v", load.cache_type_v.is_some()),
        ("--flash-attn", load.flash_attn.is_some()),
        ("--batch-size", load.batch_size.is_some()),
        ("--ubatch-size", load.ubatch_size.is_some()),
        ("--threads", load.threads.is_some()),
        ("--threads-batch", load.threads_batch.is_some()),
        ("--mmap", load.mmap),
        ("--no-mmap", load.no_mmap),
        ("--no-kv-offload", load.no_kv_offload),
        ("--no-op-offload", load.no_op_offload),
        ("--vram-buffer", load.vram_buffer.is_some()),
    ];
    let ignored: Vec<&str> = flags.iter().filter(|(_, set)| *set).map(|(name, _)| *name).collect();
    if !ignored.is_empty() {
        notes.push(format!("{} not used: {} runs the model, not this computer", ignored.join(", "), target.provider));
    }
    let samplers = &options.samplers;
    let not_openai: Vec<&str> = [("--top-k", samplers.top_k.is_some()), ("--min-p", samplers.min_p.is_some()), ("--repeat-penalty", samplers.repeat_penalty.is_some())]
        .iter()
        .filter(|(_, set)| *set)
        .map(|(name, _)| *name)
        .collect();
    if !not_openai.is_empty() {
        notes.push(format!("{} not sent: the OpenAI format has no such field", not_openai.join(", ")));
    }
    if options.think == Some(cluaiz_core::FeatureState::Off) {
        notes.push("--think off: the thinking is not shown; whether the model thinks is the provider's choice".into());
    }
    if options.effort.is_some_and(|effort| effort_name(effort).is_none()) {
        notes.push("--reasoning-effort: a provider takes only low, medium or high; not sent".into());
    }
    if options.budget_message.is_some() {
        notes.push("--reasoning-budget-message: not sent to a provider".into());
    }
    ProviderChat { target, notes }
}

/// The OpenAI `reasoning_effort` of a thinking budget, when it has one
fn effort_name(effort: cluaiz_core::ThinkingBudget) -> Option<&'static str> {
    match effort {
        cluaiz_core::ThinkingBudget::Low => Some("low"),
        cluaiz_core::ThinkingBudget::Medium => Some("medium"),
        cluaiz_core::ThinkingBudget::High => Some("high"),
        _ => None,
    }
}

/// The provider model `name` names (`<provider>/<model>`), when providers.json has that provider.
/// A providers.json that cannot be read is reported once and ignored.
pub fn find(name: &str) -> Option<Target> {
    if !name.contains('/') {
        return None;
    }
    match cluaiz_core::providers::Providers::load() {
        Ok(providers) => providers.find(name),
        Err(error) => {
            eprintln!("providers.json ignored: {error}");
            None
        }
    }
}

/// A plan for the header and `/plan`: nothing is loaded here, the provider runs the model
pub fn plan(target: &Target) -> cluaiz_plan::ExecutionPlan {
    cluaiz_plan::ExecutionPlan {
        model_id: target.full_name(),
        selected_tier: cluaiz_plan::PlacementTier::CpuOnly,
        n_gpu_layers: 0,
        n_cpu_moe: 0,
        context_window: target.context(),
        total_layers: 0,
        vram_budget_gb: 0.0,
        ram_budget_gb: 0.0,
        kv_gpu_gb: 0.0,
        kv_ram_gb: 0.0,
        disk_stream_gb: 0.0,
        kv_offload: false,
        op_offload: false,
        kv_cache_type: Default::default(),
        vram_buffer_gb: 0.0,
        ram_budget_limit_gb: 0.0,
        context_reason: format!("the context set for this model in providers.json, else {}", cluaiz_core::providers::DEFAULT_CONTEXT),
        candidates: Vec::new(),
        explanation: format!("runs at {} ({})", target.provider, target.config.base_url),
    }
}

/// What the header and `/plan` show for a provider model
pub fn summary(target: &Target) -> String {
    format!(
        "provider   {} | {}\ncontext    {} tokens\nkey        {}",
        target.provider,
        target.config.base_url,
        target.context(),
        target.config.key_source()
    )
}

/// The conversation in the OpenAI format, with the terminal's attachments
/// (`cluaiz_api::providers::openai_conversation_with_parts`)
pub fn messages(messages: Vec<ChatMessage<Vec<PreparedPart>>>) -> Result<Vec<Value>> {
    cluaiz_api::providers::openai_conversation_with_parts(messages).map_err(|e| eyre!(e))
}

/// A call being streamed: providers send its name first and its arguments in pieces
#[derive(Default)]
struct PartialCall {
    name: String,
    arguments: String,
}

impl ProviderChat {
    /// One answer, streamed into `out`; the provider's calls come back in the `<tool_call>` form
    pub async fn generate(
        &self,
        options: &ChatOptions,
        messages: Vec<ChatMessage<Vec<PreparedPart>>>,
        tools: Option<&Value>,
        out: &mut dyn output::Output,
    ) -> Result<(String, TurnReport)> {
        let target = &self.target;
        let mut body = json!({
            "model": target.model,
            "messages": self::messages(messages)?,
            "stream": true,
            "stream_options": { "include_usage": true },
        });
        // This run's flags, then settings.json, as for an installed model
        let saved = cluaiz_core::Settings::current().samplers;
        let flags = &options.samplers;
        let s = cluaiz_core::SamplerDefaults {
            temp: flags.temp.or(saved.temp),
            top_p: flags.top_p.or(saved.top_p),
            presence_penalty: flags.presence_penalty.or(saved.presence_penalty),
            frequency_penalty: flags.frequency_penalty.or(saved.frequency_penalty),
            seed: flags.seed.or(saved.seed),
            ..Default::default()
        };
        let fields: [(&str, Option<Value>); 5] = [
            ("temperature", s.temp.map(|v| json!(v))),
            ("top_p", s.top_p.map(|v| json!(v))),
            ("presence_penalty", s.presence_penalty.map(|v| json!(v))),
            ("frequency_penalty", s.frequency_penalty.map(|v| json!(v))),
            ("seed", s.seed.map(|v| json!(v))),
        ];
        for (name, value) in fields {
            if let Some(value) = value {
                body[name] = value;
            }
        }
        if let Some(max) = options.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(effort) = options.effort.and_then(effort_name) {
            body["reasoning_effort"] = json!(effort);
        }
        if let Some(tools) = tools.filter(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
            body["tools"] = tools.clone();
        }

        cluaiz_core::live::start();
        GENERATING.store(true, Ordering::SeqCst);
        let show_thinking = options.think != Some(cluaiz_core::FeatureState::Off);
        let result = self.stream(body, show_thinking, out).await;
        GENERATING.store(false, Ordering::SeqCst);
        let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.swap(false, Ordering::SeqCst);
        out.finish();
        let (answer, finish, used) = result?;
        let stop = match finish.as_str() {
            "length" => "limit",
            _ if cancelled => "cancel",
            _ => "eos",
        };
        let context = used.map(|used| (used, target.context() as usize));
        Ok((answer, TurnReport { timings: None, thinking: None, cancelled, stop, context, dropped: 0 }))
    }

    /// The answer, why it ended, and the tokens the provider counted (prompt + answer)
    async fn stream(&self, body: Value, show_thinking: bool, out: &mut dyn output::Output) -> Result<(String, String, Option<usize>)> {
        let target = &self.target;
        let (builder, key) = cluaiz_api::providers::request(&target.config, reqwest::Method::POST, &target.config.chat_url()).map_err(|e| eyre!(e))?;
        let mut response = builder
            .timeout(Duration::from_secs(24 * 3600))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| eyre!(cluaiz_api::providers::send_error(target, &e, key.as_deref())))?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let text = response.text().await.unwrap_or_default();
            return Err(eyre!(cluaiz_api::providers::error_message(target, status, &text, key.as_deref())));
        }
        // Some providers put the thinking into the text between <think> tags
        let mut filter = StreamingReasoningFilter::new(Some("<think>".into()), Some("</think>".into()), false, false);
        let mut answer = String::new();
        let mut finish = String::from("stop");
        let mut used = None;
        let mut calls: BTreeMap<u64, PartialCall> = BTreeMap::new();
        let mut pending = String::new();
        let mut tick = tokio::time::interval(Duration::from_millis(120));
        let emit = |(content, reasoning): (Option<String>, Option<String>), out: &mut dyn output::Output, answer: &mut String| {
            if let Some(r) = reasoning.filter(|r| show_thinking && !r.is_empty()) {
                out.reasoning(&r);
            }
            if let Some(c) = content.filter(|c| !c.is_empty()) {
                out.content(&c);
                answer.push_str(&c);
            }
        };
        'read: loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => chunk.map_err(|e| eyre!(cluaiz_api::providers::send_error(target, &e, key.as_deref())))?,
                _ = tick.tick() => {
                    out.tick();
                    // Ctrl+C: dropping the stream ends the request at the provider
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
                    break 'read;
                }
                let Ok(event) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(message) = event["error"]["message"].as_str() {
                    return Err(eyre!("{}: {}", target.provider, cluaiz_core::providers::redact(message, key.as_deref())));
                }
                let usage = event.get("usage").filter(|u| u.is_object()).or_else(|| event.pointer("/x_groq/usage"));
                if let Some(total) = usage.and_then(|u| u["total_tokens"].as_u64().or_else(|| Some(u["prompt_tokens"].as_u64()? + u["completion_tokens"].as_u64().unwrap_or(0)))) {
                    used = Some(total as usize);
                }
                let Some(choice) = event["choices"].get(0) else { continue };
                if let Some(reason) = choice["finish_reason"].as_str() {
                    finish = reason.to_owned();
                }
                let delta = &choice["delta"];
                for field in ["reasoning_content", "reasoning"] {
                    if let Some(thinking) = delta[field].as_str().filter(|t| show_thinking && !t.is_empty()) {
                        out.reasoning(thinking);
                    }
                }
                if let Some(text) = delta["content"].as_str() {
                    cluaiz_core::live::token();
                    emit(filter.process_token(text), out, &mut answer);
                }
                for (position, call) in delta["tool_calls"].as_array().into_iter().flatten().enumerate() {
                    let index = call["index"].as_u64().unwrap_or(position as u64);
                    let partial = calls.entry(index).or_default();
                    if let Some(name) = call["function"]["name"].as_str() {
                        partial.name.push_str(name);
                    }
                    if let Some(arguments) = call["function"]["arguments"].as_str() {
                        partial.arguments.push_str(arguments);
                    } else if call["function"]["arguments"].is_object() {
                        partial.arguments.push_str(&call["function"]["arguments"].to_string());
                    }
                    cluaiz_core::live::set_call(&partial.name, &partial.arguments);
                }
            }
        }
        emit(filter.flush_final(), out, &mut answer);
        for call in calls.into_values().filter(|c| !c.name.is_empty()) {
            let arguments = if call.arguments.trim().is_empty() { "{}" } else { call.arguments.as_str() };
            let block = format!("\n{}", cluaiz_core::tool_calls::canonical_call(&call.name, arguments));
            out.content(&block);
            answer.push_str(&block);
        }
        Ok((answer, finish, used))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(role: &str, text: &str) -> ChatMessage<Vec<PreparedPart>> {
        ChatMessage { role: role.into(), content: vec![PreparedPart::Text { text: text.into() }] }
    }

    #[test]
    fn an_attached_image_and_sound_become_content_parts() {
        let image = cluaiz_core::media::DecodedImage::new(2, 1, std::sync::Arc::from(vec![255u8, 0, 0, 0, 0, 255])).unwrap();
        let audio = cluaiz_core::media::DecodedAudio::new(16_000, 1, std::sync::Arc::from(vec![0.0f32; 160])).unwrap();
        let out = messages(vec![
            text("system", "You help."),
            ChatMessage {
                role: "user".into(),
                content: vec![PreparedPart::Text { text: "What is this?".into() }, PreparedPart::Image { image }, PreparedPart::Audio { audio }],
            },
        ])
        .unwrap();
        let parts = out[1]["content"].as_array().expect("content parts");
        assert_eq!(parts[0], json!({ "type": "text", "text": "What is this?" }));
        assert!(parts[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,iVBOR"));
        assert_eq!(parts[2]["input_audio"]["format"], "wav");
        assert_eq!(out[0]["content"], "You help.");
    }

    #[test]
    fn calls_and_results_are_paired_for_the_provider() {
        let call = cluaiz_core::tool_calls::canonical_call("read_file", r#"{"path":"a.txt"}"#);
        let out = messages(vec![
            text("system", "You help."),
            text("user", "read a.txt"),
            text("assistant", &call),
            text("tool", "hello"),
            text("assistant", "It says hello."),
        ])
        .unwrap();
        assert_eq!(out[2]["content"], Value::Null);
        assert_eq!(out[2]["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(out[3]["role"], "tool");
        assert_eq!(out[3]["tool_call_id"], out[2]["tool_calls"][0]["id"]);
        assert_eq!(out[4]["content"], "It says hello.");
    }

    #[test]
    fn a_call_without_a_result_is_answered_and_a_lone_result_is_user_text() {
        let call = cluaiz_core::tool_calls::canonical_call("run_command", r#"{"command":"ls"}"#);
        let out = messages(vec![text("user", "go"), text("assistant", &call), text("user", "stop, do this instead")]).unwrap();
        assert_eq!(out[2]["role"], "tool");
        assert_eq!(out[2]["content"], "(not run)");
        assert_eq!(out[3]["role"], "user");
        let out = messages(vec![text("user", "go"), text("tool", "orphan")]).unwrap();
        assert_eq!(out[1]["role"], "user");
        assert!(out[1]["content"].as_str().unwrap().contains("orphan"));
    }

    #[test]
    fn flags_without_meaning_at_a_provider_are_named_and_c_sets_the_context() {
        let mut config = cluaiz_core::providers::Provider::new("https://api.example.com/v1");
        config.models.insert("m".into(), cluaiz_core::providers::ProviderModel { context: Some(8192), kind: None });
        let target = Target { provider: "myapi".into(), config, model: "m".into() };
        let mut options = ChatOptions::default();
        options.load.ctx_size = Some(32768);
        options.load.vram_buffer = Some(1.0);
        options.load.threads = Some(4);
        options.samplers.top_k = Some(40);
        options.think = Some(cluaiz_core::FeatureState::Off);
        options.effort = Some(cluaiz_core::ThinkingBudget::Tokens(512));
        let chat = prepare(target, &options);
        assert_eq!(chat.target.context(), 8192, "-c above the model's limit keeps the limit");
        let notes = chat.notes.join("\n");
        for named in ["-c 32768", "--threads", "--vram-buffer", "--top-k", "--think off", "--reasoning-effort"] {
            assert!(notes.contains(named), "{named} in {notes}");
        }
        assert_eq!(effort_name(cluaiz_core::ThinkingBudget::Medium), Some("medium"));
        let plain = prepare(chat.target.clone(), &ChatOptions::default());
        assert!(plain.notes.is_empty());
    }
}
