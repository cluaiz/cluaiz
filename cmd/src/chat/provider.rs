//! A provider's model (`config/providers.json`) as the model of `cluaiz run` and `cluaiz agent`.
//! The conversation is sent in the OpenAI Chat Completions format: calls in the Engine's
//! `<tool_call>` form become `tool_calls`, each tool result answers its call by `tool_call_id`, and
//! the calls the provider returns come back in the `<tool_call>` form the agent reads.

use std::sync::atomic::Ordering;
use std::time::Duration;

use cluaiz_api::providers::{chat_body, data_of, ChatParts, open, sse_lines, Compat, Decoder, Opened, Read, Room, Sampling, Thinking};
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
        notes.push("--think off: the provider is asked not to think where it can turn thinking off; nothing of it is shown".into());
    }
    if let Some(budget) = options.effort.filter(|e| matches!(e, cluaiz_core::ThinkingBudget::Tokens(n) if *n > 0)) {
        if let Some(level) = Thinking::of("auto", budget).effort {
            notes.push(format!("--reasoning-effort {}: a provider takes a level; sent as {}", effort_label(budget), level.name()));
        }
    }
    if options.budget_message.is_some() {
        notes.push("--reasoning-budget-message: not sent to a provider".into());
    }
    ProviderChat { target, notes }
}

/// A thinking budget as the flag gave it
fn effort_label(budget: cluaiz_core::ThinkingBudget) -> String {
    match budget {
        cluaiz_core::ThinkingBudget::Tokens(n) => format!("{n} tokens"),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// `--think` as the chat's think mode
fn think_mode(options: &ChatOptions) -> &'static str {
    match options.think {
        Some(cluaiz_core::FeatureState::Off) => "off",
        Some(cluaiz_core::FeatureState::On) => "on",
        _ => "auto",
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

/// The conversation in the OpenAI format for `to`, with the terminal's attachments
/// (`cluaiz_api::providers::openai_conversation_with_parts`)
pub fn messages(messages: Vec<ChatMessage<Vec<PreparedPart>>>, to: Option<&Target>) -> Result<Vec<Value>> {
    cluaiz_api::providers::openai_conversation_with_parts(messages, to).map_err(|e| eyre!(e))
}

impl ProviderChat {
    /// One answer, streamed into `out`; the provider's calls come back in the `<tool_call>` form.
    /// The request is built and read by the same code as the server's chat
    /// (`cluaiz_api::providers`): samplers, thinking on/off and level, retries, refused fields.
    pub async fn generate(
        &self,
        options: &ChatOptions,
        messages: Vec<ChatMessage<Vec<PreparedPart>>>,
        tools: Option<&Value>,
        out: &mut dyn output::Output,
    ) -> Result<(String, TurnReport)> {
        let target = &self.target;
        // This run's flags, then settings.json, as for an installed model
        let saved = cluaiz_core::Settings::current().samplers;
        let flags = &options.samplers;
        let sampling = Sampling {
            temperature: flags.temp.or(saved.temp).map(|v| json!(v)),
            top_p: flags.top_p.or(saved.top_p).map(|v| json!(v)),
            presence_penalty: flags.presence_penalty.or(saved.presence_penalty).map(|v| json!(v)),
            frequency_penalty: flags.frequency_penalty.or(saved.frequency_penalty).map(|v| json!(v)),
            seed: flags.seed.or(saved.seed).map(|v| json!(v)),
            max_tokens: options.max_tokens,
        };
        let thinking = Thinking::of(think_mode(options), options.effort.unwrap_or_default());
        let compat = Compat::of(target);
        let parts = ChatParts { model: &target.model, messages: self::messages(messages, Some(target))?, sampling: &sampling, thinking, tools, response_format: None, extra: None };
        let body = chat_body(&compat, parts);

        cluaiz_core::live::start();
        GENERATING.store(true, Ordering::SeqCst);
        let result = self.stream(body, &compat, thinking.shown(), out).await;
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

    /// The answer, why it ended, and the tokens the provider counted (prompt + answer). An
    /// answer that gave nothing at all is asked for once more.
    async fn stream(&self, mut body: Value, compat: &Compat, show_thinking: bool, out: &mut dyn output::Output) -> Result<(String, String, Option<usize>)> {
        let target = &self.target;
        let cancel = Some(&cluaiz_core::GLOBAL_CANCEL_SIGNAL);
        // The agent makes room itself when the context is full (`make_room`)
        let room = Room { drop_turns: false };
        let mut empty_tries = 0;
        loop {
            let (mut response, key) = match open(target, &mut body, room, cancel).await.map_err(|e| eyre!(e))? {
                (Opened::Stream { response, key }, _) => (response, key),
                // A call the provider refused goes on as the answer: the agent answers it
                (Opened::Calls(calls), _) => {
                    out.content(&calls);
                    return Ok((calls, "tool_calls".into(), None));
                }
            };
            // Thinking between <think> tags in the text (and Gemini's <thought>, read as <think>)
            let mut filter = StreamingReasoningFilter::new(Some("<think>".into()), Some("</think>".into()), false, false);
            let mut answer = String::new();
            let mut decoder = Decoder::new(compat);
            let mut pending: Vec<u8> = Vec::new();
            let mut tick = tokio::time::interval(Duration::from_millis(120));
            let mut broken: Option<String> = None;
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
                    chunk = response.chunk() => chunk,
                    _ = tick.tick() => {
                        out.tick();
                        // Ctrl+C: dropping the stream ends the request at the provider
                        if cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) {
                            break;
                        }
                        continue;
                    }
                };
                let chunk = match chunk {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => break,
                    Err(error) => {
                        broken = Some(cluaiz_api::providers::send_error(target, &error, key.as_deref()));
                        break;
                    }
                };
                for line in sse_lines(&mut pending, &chunk) {
                    let Some(data) = data_of(&line) else { continue };
                    match decoder.read(data) {
                        Read::Done => break 'read,
                        Read::Nothing => {}
                        Read::Error { message, .. } => {
                            broken = Some(format!("{}: {}", target.provider, cluaiz_core::providers::redact(&message, key.as_deref())));
                            break 'read;
                        }
                        Read::Piece { thinking, text, call } => {
                            if let Some(thinking) = thinking.filter(|_| show_thinking) {
                                out.reasoning(&thinking);
                            }
                            if let Some(text) = text {
                                cluaiz_core::live::token();
                                emit(filter.process_token(&text), out, &mut answer);
                            }
                            if let Some((name, arguments)) = call {
                                cluaiz_core::live::set_call(&name, &arguments);
                            }
                        }
                    }
                }
            }
            let cancelled = cluaiz_core::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst);
            if !decoder.produced() && !cancelled && empty_tries < 1 {
                empty_tries += 1;
                continue;
            }
            if let Some(message) = broken {
                return Err(eyre!(message));
            }
            let finished = decoder.finish(&target.provider);
            emit(filter.process_token(&finished.text), out, &mut answer);
            emit(filter.flush_final(), out, &mut answer);
            for block in finished.calls {
                let block = format!("\n{block}");
                out.content(&block);
                answer.push_str(&block);
            }
            let used = finished.usage.map(|(_, total)| total);
            return Ok((answer, finished.finish.unwrap_or_else(|| "stop".into()), used));
        }
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
        ], None)
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
        ], None)
        .unwrap();
        assert_eq!(out[2]["content"], Value::Null);
        assert_eq!(out[2]["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(out[3]["role"], "tool");
        assert_eq!(out[3]["tool_call_id"], out[2]["tool_calls"][0]["id"]);
        assert_eq!(out[4]["content"], "It says hello.");
    }

    #[test]
    fn a_providers_call_goes_back_to_it_with_its_signature() {
        // As Gemini 3 streams a call: its id and thought signature beside the function
        let mut calls = cluaiz_api::providers::StreamedCalls::default();
        calls.add(&json!({ "tool_calls": [{ "index": 0, "id": "call_77", "type": "function", "extra_content": { "google": { "thought_signature": "SIG" } }, "function": { "name": "read_file", "arguments": "{\"path\":\"a.txt\"}" } }] }));
        let block = calls.blocks("gemini").remove(0);
        let conversation = || vec![text("user", "read a.txt"), text("assistant", &block), text("tool", "hello")];
        let gemini = Target { provider: "gemini".into(), config: cluaiz_core::providers::Provider::new("https://generativelanguage.googleapis.com/v1beta/openai"), model: "m".into() };
        let out = messages(conversation(), Some(&gemini)).unwrap();
        assert_eq!(out[1]["tool_calls"][0]["extra_content"]["google"]["thought_signature"], "SIG");
        assert_eq!(out[1]["tool_calls"][0]["id"], "call_77");
        assert_eq!(out[2]["tool_call_id"], "call_77");
        // Another provider gets only the name and the arguments
        let groq = Target { provider: "groq".into(), config: cluaiz_core::providers::Provider::new("https://api.groq.com/openai/v1"), model: "m".into() };
        let out = messages(conversation(), Some(&groq)).unwrap();
        assert!(out[1]["tool_calls"][0].get("extra_content").is_none());
        assert_eq!(out[1]["tool_calls"][0]["id"], "call_1");
    }

    #[test]
    fn a_call_without_a_result_is_answered_and_a_lone_result_is_user_text() {
        let call = cluaiz_core::tool_calls::canonical_call("run_command", r#"{"command":"ls"}"#);
        let out = messages(vec![text("user", "go"), text("assistant", &call), text("user", "stop, do this instead")], None).unwrap();
        assert_eq!(out[2]["role"], "tool");
        assert_eq!(out[2]["content"], "(not run)");
        assert_eq!(out[3]["role"], "user");
        let out = messages(vec![text("user", "go"), text("tool", "orphan")], None).unwrap();
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
        assert!(notes.contains("sent as low"), "{notes}");
        let plain = prepare(chat.target.clone(), &ChatOptions::default());
        assert!(plain.notes.is_empty());
    }
}
