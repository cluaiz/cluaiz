use super::tokenizer::tokenize;
use crate::ffi::llama_cpp;
use crate::native::context::SafeBatch;
use crate::native::core::{ContextExceeded, ContextReport, NativeLlama};
use crate::native::media::{MediaInput, Projector};
use cluaiz_models::types::StructuralDNA;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use tracing::info;

pub(crate) struct StreamOptions<'a> {
    pub media: Option<(&'a mut MediaInput, &'a mut Projector)>,
    pub skip_reasoning: Option<&'a std::sync::atomic::AtomicBool>,
    /// The request's stop flag, read between prompt chunks too (a token callback only runs once
    /// the prompt is processed)
    pub cancel: Option<&'a std::sync::atomic::AtomicBool>,
}

/// Prompt chunk of an idle-time job: a request that arrives waits for one chunk at most. Not
/// smaller: with layers in RAM every decode has a fixed cost (weights copied to the GPU, or a
/// CPU pass under 32 tokens), so small chunks would multiply it without stopping sooner.
const BACKGROUND_PROMPT_CHUNK: usize = 64;
/// An answer shorter than this does not update the decode speed Auto effort uses (start-up costs
/// dominate a few tokens)
const MIN_TOKENS_FOR_SPEED: i32 = 32;

struct SafeSampler {
    sampler: *mut std::ffi::c_void,
}

impl Drop for SafeSampler {
    fn drop(&mut self) {
        if !self.sampler.is_null() {
            unsafe {
                crate::ffi::llama_cpp::llama_sampler_free(self.sampler);
            }
        }
    }
}

pub fn stream_tokens(
    llama: &mut NativeLlama,
    prompt: &str,
    max_tokens: usize,
    dna: &StructuralDNA,
    last_prefilled_tokens: &[i32],
    callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
) -> anyhow::Result<Vec<i32>> {
    stream_tokens_with_media(
        llama,
        prompt,
        max_tokens,
        dna,
        last_prefilled_tokens,
        callback,
        StreamOptions { media: None, skip_reasoning: None, cancel: None },
    )
}

/// Shared template/sampler loop; only prefill differs for typed media requests.
pub(crate) fn stream_tokens_with_media(
    llama: &mut NativeLlama,
    prompt: &str,
    max_tokens: usize,
    dna: &StructuralDNA,
    last_prefilled_tokens: &[i32],
    mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    options: StreamOptions<'_>,
) -> anyhow::Result<Vec<i32>> {
    let StreamOptions { media, skip_reasoning, cancel } = options;
    let is_media = media.is_some();
    if is_media && !last_prefilled_tokens.is_empty() {
        anyhow::bail!("Text-only prefix reuse is unavailable for media requests");
    }
    // SAFETY: exclusively borrowed, initialized model/context; native batch/sampler
    // owners retain their allocations through every decode and release them once.
    unsafe {
        // 🛑 ROOT FIX: Reset interrupt signal when entering generation to ensure pivot works!
        if !is_media {
            llama.interrupt_signal.store(false, Ordering::SeqCst);
        }
        llama.reset_timings();
        tracing::debug!(
            "🔥 [NativeStream] stream_tokens CALLED! prompt len={}",
            prompt.len()
        );

        let is_pivot = prompt.starts_with("[PIVOT_CONTINUE]");
        let actual_prompt = if is_pivot {
            prompt
                .trim_start_matches("[PIVOT_CONTINUE]")
                .trim_start()
                .to_string()
        } else {
            prompt.to_string()
        };

        // 🧬 Dynamic In-Memory Request Overrides & Native Chat Message Parsing
        let mut structured_messages: Vec<(String, String)> = Vec::new();
        // Debug: the rendered prompt goes into the context report (template tests)
        let mut return_prompt = false;
        // An idle-time job (card, summary): its prompt goes in small chunks, so it stops quickly
        let mut background = false;
        let (req_samplers, req_think_mode, req_reasoning_effort, req_output, req_budget_message, req_kv_reuse, req_tools, keep_prefix, req_stops, req_logit_bias) =
            if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&actual_prompt) {
                if envelope.is_object()
                    && (envelope.get("messages").is_some()
                        || envelope.get("pivot_prompt").is_some())
                {
                    if let Some(pivot) = envelope.get("pivot_prompt").and_then(|p| p.as_str()) {
                        structured_messages.push(("user".to_string(), pivot.to_string()));
                    } else if let Some(msgs) = envelope.get("messages").and_then(|m| m.as_array()) {
                        for m in msgs {
                            let role = m
                                .get("role")
                                .and_then(|r| r.as_str())
                                .unwrap_or("user")
                                .to_string();
                            let content = m
                                .get("content")
                                .and_then(|c| c.as_str())
                                .unwrap_or("")
                                .to_string();
                            structured_messages.push((role, content));
                        }
                    }
                    let samplers = envelope.get("samplers").cloned();
                    let think_mode = envelope
                        .get("think_mode")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());
                    let reasoning_effort =
                        envelope.get("reasoning_effort").cloned().and_then(|e| {
                            serde_json::from_value::<cluaiz_core::ThinkingBudget>(e).ok()
                        });
                    // The answer's shape: `grammar` (GBNF) or `response_format` (JSON / JSON schema)
                    let grammar = cluaiz_core::OutputConstraint::from_request(
                        envelope.get("grammar").and_then(|g| g.as_str()),
                        envelope.get("response_format"),
                    );
                    let budget_message = envelope
                        .get("reasoning_budget_message")
                        .and_then(|m| m.as_str())
                        .filter(|m| !m.is_empty())
                        .map(|m| m.to_string());
                    let kv_reuse = envelope
                        .get("kv_reuse")
                        .and_then(|m| m.as_str())
                        .map(|m| m.to_string());
                    // Function schemas the model's own template renders (native tool calls), and
                    // whether calls are read from the answer (`parse_tools: false` keeps the
                    // same prompt for a job that wants plain text, like an idle-time card), and
                    // the request's `tool_choice` ("none", "required"; missing = "auto")
                    let tools = envelope
                        .get("tools")
                        .filter(|t| t.as_array().is_some_and(|a| !a.is_empty()))
                        .cloned()
                        .map(|tools| (
                            tools,
                            envelope.get("parse_tools").and_then(|p| p.as_bool()) != Some(false),
                            envelope.get("tool_choice").and_then(|c| c.as_str()).map(str::to_string),
                        ));
                    // Thinking off without changing the start of the prompt (a job that reuses a
                    // chat's KV): see below
                    let keep_prefix = envelope.get("thinking_off_keeps_prefix").and_then(|k| k.as_bool()) == Some(true);
                    return_prompt = envelope.get("return_prompt").and_then(|k| k.as_bool()) == Some(true);
                    background = envelope.get("background").and_then(|k| k.as_bool()) == Some(true);
                    // OpenAI `stop`: the answer ends before the first of these strings (not sent)
                    let stops: Vec<String> = envelope
                        .get("stop")
                        .and_then(|s| s.as_array())
                        .map(|s| s.iter().filter_map(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string).collect())
                        .unwrap_or_default();
                    // OpenAI `logit_bias` as [token, bias] pairs (`ChatEnvelope::logit_bias`)
                    let logit_bias: Vec<(i32, f32)> = envelope
                        .get("logit_bias")
                        .and_then(|b| serde_json::from_value(b.clone()).ok())
                        .unwrap_or_default();
                    (
                        samplers,
                        think_mode,
                        reasoning_effort,
                        grammar,
                        budget_message,
                        kv_reuse,
                        tools,
                        keep_prefix,
                        stops,
                        logit_bias,
                    )
                } else {
                    (None, None, None, None, None, None, None, false, Vec::new(), Vec::new())
                }
            } else {
                (None, None, None, None, None, None, None, false, Vec::new(), Vec::new())
            };

        let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
        let has_loaded_cache = !last_prefilled_tokens.is_empty();
        // A media prompt matches the KV against its own chunk layout during evaluation
        if !is_pivot && !is_media {
            if has_loaded_cache {
                // Keep the loaded KV cache sequence, only remove anything AFTER the loaded tokens
                let loaded_len = last_prefilled_tokens.len() as i32;
                llama_cpp::llama_memory_seq_rm(mem, 0, loaded_len, -1);
            } else {
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
            }
        }

        // Thinking tags: llama.cpp's template engine read them once at load (NativeLlama::load stores
        // exactly its result in the DNA, empty when the template has none); no name-based filtering
        let think_start_tag = dna.think_tag_schema.clone();
        let think_end_tag = dna.think_end_schema.clone();

        let tm_str = req_think_mode.as_deref().unwrap_or("auto").to_lowercase();

        let mut enable_thinking = match tm_str.as_str() {
            "off" | "false" | "0" | "hide" => false,
            _ => true,
        };
        // A job that wants no thinking but the chat's own prompt start (to reuse its KV) sends the
        // turn's think mode and `thinking_off_keeps_prefix`. Thinking is turned off in the prompt,
        // except where the template writes the switch at the very start (Gemma 4 in its system
        // turn): there it stays on with a zero thinking budget.
        let mut zero_budget = false;
        if keep_prefix && enable_thinking && !think_start_tag.is_empty() {
            let probe = [("system", "S7xq"), ("user", "U9zk")];
            let templates = llama.chat_templates_for(dna.chat_template.as_deref());
            let on = crate::native::templater::apply_chat_template(llama.model_ptr, templates, dna.chat_template.as_deref(), &probe, true, true);
            let off = crate::native::templater::apply_chat_template(llama.model_ptr, templates, dna.chat_template.as_deref(), &probe, true, false);
            if let (Ok(on), Ok(off)) = (on, off) {
                // The user turn ends the same way in both: compare everything before it
                let before = |p: &str| p.find("U9zk").map(|i| p[..i].to_string()).unwrap_or_default();
                if before(&on) != before(&off) {
                    zero_budget = true;
                    info!("🧠 [NativeStream] Thinking stays on in the prompt (the template switches it at the start); budget 0");
                } else {
                    enable_thinking = false;
                }
            }
        }

        if structured_messages.is_empty() {
            // Plain text (CLI / TUI): one user message through the model's own template.
            // No caller sends pre-formatted prompts, so the text is never guessed to be one by its markers.
            structured_messages.push(("user".to_string(), actual_prompt.clone()));
        }
        // Calls a provider's model made earlier in the chat keep what that provider sent with
        // them (a thought signature); this model sees only their names and arguments
        let mut structured_messages = cluaiz_core::tool_calls::without_provider_data(&structured_messages);

        let vocab = llama_cpp::llama_model_get_vocab(llama.model_ptr);
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

        if n_vocab <= 0 {
            return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
        }

        let n_ctx = llama.n_ctx as usize;
        let shifting = cluaiz_core::Settings::current()
            .optimization
            .context_shifting;
        // llama-server turns context shift and cache reuse off when the memory cannot move positions
        // (sliding-window / recurrent caches) and for multimodal prompts; quantized keys cannot move
        // either, though llama_memory_can_shift says they can
        let can_shift = !is_media && llama.keys_can_shift && llama_cpp::llama_memory_can_shift(mem);
        // BOS the llama.cpp way: its chat template engine strips the BOS text from the rendered prompt and
        // the tokenizer adds the BOS token when the model asks for it (llama_vocab_get_add_bos).
        // A continuation (pivot) is appended to the existing context, so it gets none.
        let add_bos = llama_cpp::llama_vocab_get_add_bos(vocab);
        let add_special = !is_pivot && add_bos;
        let gen_reserve = cluaiz_core::answer_reserve(max_tokens, n_ctx);
        // Room kept free when a prompt is fitted: the answer's reserve, twice that for a model that
        // thinks before answering (the thinking budget is taken from the same room)
        let fit_reserve = if enable_thinking && !think_start_tag.is_empty() {
            (gen_reserve * 2).min(n_ctx / 2)
        } else {
            gen_reserve
        };
        let mut report = ContextReport {
            n_ctx,
            stop: "eos",
            kv_reuse: "prefix",
            ..Default::default()
        };

        // Native tool calls: the template renders the tools and the tool turns in the model's own
        // format, and llama.cpp gives the grammar and parser for its calls (llama-server's path).
        // Image and audio prompts too: llama-server renders the tools the same way for a multimodal
        // request (the media parts are markers in the text). Continuations keep the inline form.
        let (native_tools, parse_tools, tool_choice) = match req_tools.filter(|_| !is_pivot) {
            Some((tools, parse, choice)) => (Some(tools), parse, choice),
            None => (None, false, None),
        };
        if native_tools.is_some() && llama.chat_tools_for(dna.chat_template.as_deref()).is_none() {
            tracing::warn!("⚠️ [NativeTools] llama.cpp could not parse the template for tool use; tools go inline");
        }
        // Without tools the answer is read by llama.cpp's parser too, as llama-server reads every
        // answer: a format with more than thinking tags (gpt-oss channels) leaves only the text
        let parse_answer = native_tools.is_none() && !is_media && !is_pivot
            && llama.chat_tools_for(dna.chat_template.as_deref()).is_some();
        // The answer's JSON schema goes into the template call, with or without tools, as llama-server
        // passes response_format (server-common.cpp `inputs.json_schema`): llama.cpp then builds one
        // strict grammar for the thinking block, the answer and any calls
        // (common/chat-auto-parser-generator.cpp). Without it, a model that opens thinking itself
        // (Qwen3: the prompt does not hold the start tag) had the schema grammar from its first token
        // and could not think. A GBNF grammar with tools is refused by the API.
        // Not while tools can be called (owner decision, 2026-11-25): llama.cpp's one strict grammar
        // let small models write the schema's JSON instead of a call (run 68 weather question: 10 / 10
        // get_weather calls before 9dde6a3e, 0 / 10 after, answers like {"age":25,"name":"get_weather"}).
        // With callable tools the answer is not constrained, as before 9dde6a3e.
        let tools_callable = native_tools.is_some() && tool_choice.as_deref() != Some("none");
        let template_schema = match &req_output {
            Some(cluaiz_core::OutputConstraint::JsonSchema(schema)) if !tools_callable && (native_tools.is_some() || parse_answer) => Some(schema.clone()),
            _ => None,
        };
        // What llama.cpp returned for the last native render (grammar, triggers, parser)
        let native_apply = std::cell::RefCell::new(None::<serde_json::Value>);
        let render = |llama: &NativeLlama, messages: &[(String, String)]| {
            if let Some(tools) = &native_tools {
                if let Some(chat_tools) = llama.chat_tools(dna.chat_template.as_deref()) {
                    let mut request = serde_json::json!({
                        "messages": cluaiz_core::tool_calls::openai_messages(messages),
                        "tools": tools,
                        "enable_thinking": enable_thinking,
                    });
                    if let Some(choice) = &tool_choice {
                        request["tool_choice"] = serde_json::json!(choice);
                    }
                    if let Some(schema) = &template_schema {
                        request["json_schema"] = schema.clone();
                    }
                    match chat_tools.apply(&request) {
                        Ok(apply) if apply["supports_tools"].as_bool() == Some(false) => {
                            tracing::info!("🧰 [NativeTools] The template does not render tools; they go inline")
                        }
                        Ok(apply) if apply["prompt"].as_str().is_some_and(|p| {
                            // A template that loses a message falls back to the inline form
                            let missing = cluaiz_core::tool_calls::missing_messages(p, messages);
                            if !missing.is_empty() {
                                tracing::warn!("⚠️ [NativeTools] The tool template dropped messages {:?}; tools go inline", missing);
                            }
                            !p.trim().is_empty() && missing.is_empty()
                        }) => {
                            let prompt = apply["prompt"].as_str().unwrap_or_default().to_string();
                            tracing::debug!("🧰 [NativeTools] Rendered prompt:\n{}", prompt);
                            *native_apply.borrow_mut() = Some(apply);
                            return Ok(prompt);
                        }
                        Ok(_) => tracing::warn!("⚠️ [NativeTools] Empty prompt from the tool template; tools go inline"),
                        Err(e) => tracing::warn!("⚠️ [NativeTools] Tool template failed ({}); tools go inline", e),
                    }
                }
                // Inline fallback: the schemas in the system message, calls as <tool_call> blocks
                *native_apply.borrow_mut() = None;
                let schemas: Vec<serde_json::Value> = tools.as_array().cloned().unwrap_or_default();
                let block = cluaiz_core::tool_calls::inline_tools(&schemas);
                let mut with_tools = messages.to_vec();
                match with_tools.first_mut() {
                    Some((role, content)) if role.eq_ignore_ascii_case("system") => {
                        *content = format!("{content}\n\n{block}");
                    }
                    _ => with_tools.insert(0, ("system".to_string(), block)),
                }
                return render_inline(llama, dna, &with_tools, enable_thinking, is_pivot, &think_end_tag);
            }
            let prompt = render_inline(llama, dna, messages, enable_thinking, is_pivot, &think_end_tag)?;
            *native_apply.borrow_mut() = parse_answer
                .then(|| llama.chat_tools(dna.chat_template.as_deref()))
                .flatten()
                .and_then(|chat_tools| {
                    let mut request = serde_json::json!({
                        "messages": cluaiz_core::tool_calls::openai_messages(messages),
                        "enable_thinking": enable_thinking,
                    });
                    if let Some(schema) = &template_schema {
                        request["json_schema"] = schema.clone();
                    }
                    chat_tools
                        .apply(&request)
                        .or_else(|e| match request.as_object_mut().and_then(|fields| fields.remove("json_schema")) {
                            // A schema llama.cpp cannot turn into a grammar: the parser stays, as before
                            // e654894f, and the schema grammar below says whether it applies
                            Some(_) => {
                                tracing::warn!("📝 [NativeTemplate] The template did not take the JSON schema ({}); answer parser kept without it", e);
                                chat_tools.apply(&request)
                            }
                            None => Err(e),
                        })
                        .map_err(|e| tracing::debug!("📝 [NativeTemplate] No answer parser: {}", e))
                        .ok()
                })
                // Content-only: the answer is the text; a render unlike the prompt: its parser
                // would read another conversation
                .filter(|apply| {
                    let same = apply["prompt"].as_str() == Some(prompt.as_str());
                    tracing::debug!("📝 [NativeTemplate] Answer parser: format {} | same prompt {}", apply["format"], same);
                    apply["format_id"].as_i64().is_some_and(|id| id != 0) && same
                });
            Ok(prompt)
        };
        let mut formatted_prompt = render(llama, &structured_messages)?;
        // A message the template did not show (it has no place for that role or field)
        if !is_media {
            report.template_dropped = cluaiz_core::tool_calls::missing_messages(&formatted_prompt, &structured_messages);
            if !report.template_dropped.is_empty() {
                tracing::warn!("⚠️ [NativeTemplate] The chat template did not show messages {:?}", report.template_dropped);
            }
        }
        let mut prompt_tokens = Vec::new();
        if !is_media {
            prompt_tokens = tokenize(vocab, &formatted_prompt, add_special, true)?;
            // Context shifting before the request: the oldest whole turns go until the prompt and the
            // answer's room fit, counted with the model's own tokenizer. A continuation (pivot) only
            // appends to the conversation already in the context, so it is fitted below by a shift.
            // Results of this request are shortened in batches. The client sends the whole history
            // each turn and the Engine shortens it again the same way, so batches keep the shortened
            // part the same for several turns: the next steps append to the cached prompt instead
            // of each changing it again, which costs a whole new prefill
            const SHORTEN_BATCH: usize = 4;
            let mut shortened = 0usize;
            while !is_pivot {
                if prompt_tokens.len() + fit_reserve <= n_ctx {
                    break;
                }
                let mut sizes = structured_messages
                    .iter()
                    .map(|(role, content)| {
                        Ok((role.as_str(), tokenize(vocab, content, false, false)?.len()))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                // The template adds tokens around every message (role markers, separators); spread
                // them over the messages, or a long chat frees too little per message and drops
                // nearly everything
                let content_tokens: usize = sizes.iter().map(|(_, n)| n).sum();
                let per_message = prompt_tokens.len().saturating_sub(content_tokens) / sizes.len().max(1);
                // By their own sizes: spread, the tool schemas would count as earlier turns that can go,
                // and the note that replaces a result would look long
                let earlier_result = cluaiz_core::oldest_earlier_result(&sizes);
                let step_result = cluaiz_core::shorten_a_step_result(&structured_messages);
                let room = cluaiz_core::room_to_keep(&sizes, prompt_tokens.len(), n_ctx, fit_reserve, gen_reserve);
                for (_, n) in &mut sizes {
                    *n += per_message;
                }
                let overflow = (prompt_tokens.len() + room).saturating_sub(n_ctx);
                let batch_open = shortened % SHORTEN_BATCH != 0;
                let Some(target) = (overflow > 0 || batch_open)
                    .then(|| cluaiz_core::tokens_to_free(shifting, n_ctx, overflow.max(1), can_shift))
                    .flatten()
                else {
                    break;
                };
                if let Some(at) = earlier_result {
                    report.dropped_tokens += sizes[at].1;
                    report.truncated = true;
                    structured_messages[at].1 = cluaiz_core::EARLIER_RESULT_NOTE.to_string();
                    formatted_prompt = render(llama, &structured_messages)?;
                    prompt_tokens = tokenize(vocab, &formatted_prompt, add_special, true)?;
                    continue;
                }
                let (mut n_drop, mut freed) = cluaiz_core::oldest_turns_to_drop(&sizes, target);
                let mut start = usize::from(structured_messages[0].0.eq_ignore_ascii_case("system"));
                if n_drop == 0 {
                    // Only the current request is left. What its tools returned is worth more than
                    // the extra room for thinking: steps go only when the answer's own room is short.
                    let short = (prompt_tokens.len() + gen_reserve).saturating_sub(n_ctx);
                    // Results are shortened before whole steps go: the calls and the start of what
                    // they returned stay, so the model sees what it has done and what it found
                    if let Some((at, shorter)) = step_result.filter(|_| short > 0 || batch_open) {
                        shortened += 1;
                        info!(
                            "🌊 [Context] Shortened the result of step message {} ({} to {} characters); its call stays",
                            at,
                            structured_messages[at].1.chars().count(),
                            shorter.chars().count()
                        );
                        report.truncated = true;
                        structured_messages[at].1 = shorter;
                        formatted_prompt = render(llama, &structured_messages)?;
                        prompt_tokens = tokenize(vocab, &formatted_prompt, add_special, true)?;
                        continue;
                    }
                    (start, n_drop, freed) = cluaiz_core::oldest_steps_to_drop(&sizes, short);
                    if short == 0 || n_drop == 0 {
                        break;
                    }
                }
                structured_messages.drain(start..start + n_drop);
                report.dropped_messages += n_drop;
                report.dropped_tokens += freed;
                report.truncated = true;
                formatted_prompt = render(llama, &structured_messages)?;
                prompt_tokens = tokenize(vocab, &formatted_prompt, add_special, true)?;
            }
            if report.dropped_messages > 0 {
                info!(
                    "🌊 [Context] Dropped the {} oldest messages ({} tokens) so the prompt ({} tokens) fits {} tokens of context ({:?})",
                    report.dropped_messages, report.dropped_tokens, prompt_tokens.len(), n_ctx, shifting
                );
            }
            if !is_pivot && prompt_tokens.len() >= n_ctx {
                return Err(ContextExceeded(format!(
                    "request ({} tokens) exceeds the available context size ({} tokens), try increasing it",
                    prompt_tokens.len(),
                    n_ctx
                ))
                .into());
            }
        }

        let suppress_thinking = keep_prefix || match tm_str.as_str() {
            "off" | "false" | "0" | "hide" => true,
            "on" | "true" | "1" | "show" => false,
            _ => false, // "auto" lets model natural tags emit
        };

        // Thinking budget ("Effort"); the token count is known once the prompt length is
        let thinking_budget = if zero_budget {
            cluaiz_core::ThinkingBudget::Tokens(0)
        } else {
            req_reasoning_effort.unwrap_or_default()
        };

        let mut in_think_block = false;
        let mut think_tokens_count = 0usize;

        // Does the rendered prompt itself end inside an open thinking block (the template wrote the start
        // tag)? Then the model never prints that tag, so it is sent first and every reader (API stream
        // filter, non-streaming split) sees the block start where it really is. Decided from the rendered
        // prompt, not from the template's source, so it holds for any template.
        let last_tag = |tag: &str| {
            if tag.is_empty() {
                None
            } else {
                formatted_prompt.rfind(tag)
            }
        };
        let prompt_opens_thinking = !is_pivot
            && match (last_tag(&think_start_tag), last_tag(&think_end_tag)) {
                (Some(start), Some(end)) => start > end,
                (Some(_), None) => true,
                _ => false,
            };
        // Recent output text: a tag can arrive split over several tokens (Gemma 4: "<|channel>" + "thought")
        let mut tag_window = String::new();
        let tag_window_len = think_start_tag.len().max(think_end_tag.len());

        let mut safe_batch = SafeBatch {
            batch: llama_cpp::llama_batch_init(llama.n_batch as i32, 0, 1),
        };
        // Wall clock of the prompt decodes (with MTP the prompt numbers are this, see below)
        let mut prefill_started = std::time::Instant::now();
        let (full_prompt_tokens, mut n_cur) = if let Some((input, projector)) = media {
            let prefill = input.evaluate(projector, llama, max_tokens, &mut structured_messages, render)?;
            report = prefill.context;
            (prefill.text_tokens, prefill.positions)
        } else {
            // `cached` = the tokens the KV cache holds (the previous generation's result)
            let mut cached = last_prefilled_tokens.to_vec();
            let cached_before = cached.len();
            // Tokens the prompt shares with the cache from the start (before any reuse rule)
            let mut common_prefix = 0usize;
            let n_past = if is_pivot {
                // A continuation is appended after what the context already holds; when it does not
                // fit, the oldest tokens after BOS are shifted out first (llama-server's context shift)
                let mut start = (llama_cpp::llama_memory_seq_pos_max(mem, 0) + 1).max(0) as usize;
                if cached.len() != start {
                    // The cache's tokens are unknown (an earlier generation failed): start from it alone
                    llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                    cached.clear();
                    start = 0;
                }
                let needed = (start + prompt_tokens.len() + fit_reserve).saturating_sub(n_ctx);
                let n_keep = usize::from(add_bos);
                if needed > 0 && can_shift {
                    if let Some(discard) =
                        cluaiz_core::shift_discard(shifting, n_ctx, start.saturating_sub(n_keep))
                    {
                        let n_discard = discard.max(needed).min(start - n_keep - 1);
                        crate::native::context::shift_context(
                            llama.ctx_ptr,
                            start as i32,
                            n_keep as i32,
                            n_discard as i32,
                        );
                        cached.drain(n_keep..n_keep + n_discard);
                        start -= n_discard;
                        // Positions moved: no checkpoint describes this KV any more
                        llama.checkpoints.clear();
                        report.shifted_tokens += n_discard;
                        report.truncated = true;
                        info!(
                            "🌊 [Context] Shifted out {} tokens so the continuation fits {} tokens of context",
                            n_discard, n_ctx
                        );
                    }
                }
                if start + prompt_tokens.len() >= n_ctx {
                    anyhow::bail!(
                        "request ({} tokens) exceeds the available context size ({} tokens), try increasing it",
                        start + prompt_tokens.len(),
                        n_ctx
                    );
                }
                llama.checkpoints.retain(|c| c.n_tokens as usize <= start);
                start
            } else {
                // RAM prompt cache (llama-server `--cache-ram`, `prompt_cache.rs`): before the context is
                // overwritten its state is kept, and an earlier prompt that shares more with this one is
                // restored. Only for a plain KV: a sliding-window or recurrent state and MTP's own draft
                // context do not come back whole from one sequence state.
                let plain_kv = llama_cpp::llama_model_n_swa(llama.model_ptr) == 0
                    && !llama_cpp::llama_model_is_recurrent(llama.model_ptr)
                    && !llama_cpp::llama_model_is_hybrid(llama.model_ptr)
                    && llama.mtp.is_none();
                if plain_kv && llama.prompt_cache.enabled() {
                    let lcp = crate::native::prompt_cache::common_prefix(&cached, &prompt_tokens);
                    // The context will lose its tail: keep it first, as llama-server keeps an idle slot
                    if lcp < cached.len() && !cached.is_empty() {
                        let ctx = llama.ctx_ptr;
                        let size = llama_cpp::llama_state_seq_get_size_ext(ctx, 0, 0);
                        if size > 0 {
                            let mut data = vec![0_u8; size];
                            if llama_cpp::llama_state_seq_get_data_ext(ctx, data.as_mut_ptr(), size, 0, 0) == size {
                                llama.prompt_cache.put(cached.clone(), data);
                            }
                        }
                    }
                    // Chosen after keeping the context: keeping may drop the oldest entries
                    let best = llama.prompt_cache.best(&prompt_tokens, &cached);
                    if let Some(entry) = best.and_then(|i| llama.prompt_cache.take(i)) {
                        llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                        let restored = llama_cpp::llama_state_seq_set_data_ext(llama.ctx_ptr, entry.data.as_ptr(), entry.data.len(), 0, 0) == entry.data.len();
                        llama.checkpoints.clear();
                        if restored {
                            info!(
                                "🗂️ [PromptCache] Restored an earlier prompt: {} tokens shared, against {} with the last one ({:.1} MB state, {} kept)",
                                crate::native::prompt_cache::common_prefix(&entry.tokens, &prompt_tokens),
                                lcp,
                                entry.data.len() as f64 / (1024.0 * 1024.0),
                                llama.prompt_cache.len()
                            );
                            cached = entry.tokens;
                        } else {
                            // The KV is empty now: start from nothing
                            llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                            cached.clear();
                        }
                    }
                }
                // Reuse the KV cache: the common prefix, then (llama-server `--cache-reuse`) shared chunks
                // after it, such as the history left after old turns were dropped
                let mut n_past = cached
                    .iter()
                    .zip(&prompt_tokens)
                    .take_while(|(a, b)| a == b)
                    .count();
                common_prefix = n_past;
                // Moving chunks keeps KV that was computed with the text before it, which is
                // right only when that text was history dropped to fit. After an edit such as
                // unloading a tool, the text after it must be computed again, or the removed
                // text keeps its influence. So: chunks only when this request dropped history
                // or asked for `kv_reuse: "chunk"`, never with `kv_reuse: "prefix"`.
                let chunk_reuse = match req_kv_reuse.as_deref() {
                    Some("chunk") => true,
                    Some("prefix") => false,
                    _ => report.dropped_messages > 0,
                };
                if chunk_reuse && can_shift && n_past < cached.len() && n_past < prompt_tokens.len() {
                    let prefix = n_past;
                    n_past = crate::native::context::reuse_cache_chunks(
                        llama.ctx_ptr,
                        &mut cached,
                        &prompt_tokens,
                        n_past,
                        CACHE_REUSE_MIN_CHUNK,
                    );
                    report.kv_reuse = "chunk";
                    report.reused_chunk_tokens = n_past - prefix;
                }
                // The sampler needs fresh logits of the last prompt token: at least one is processed
                n_past = n_past.min(prompt_tokens.len().saturating_sub(1));
                // A sliding-window cache holds only the latest positions; when the kept prefix needs ones
                // it no longer has, it restarts at the newest checkpoint inside the prefix, else the
                // prompt is processed again (llama-server's context checkpoints)
                if n_past > 0 {
                    let n_swa = llama_cpp::llama_model_n_swa(llama.model_ptr).max(1);
                    if llama_cpp::llama_memory_seq_pos_min(mem, 0) > (n_past as i32 - n_swa).max(0)
                    {
                        let ctx = llama.ctx_ptr;
                        n_past = llama
                            .checkpoints
                            .iter()
                            .rev()
                            .find(|c| c.n_tokens > 0 && c.n_tokens as usize <= n_past)
                            .filter(|c| crate::native::context::load_checkpoint(ctx, c))
                            .map_or(0, |c| c.n_tokens as usize);
                        if n_past > 0 {
                            info!("🧠 [Context] Restarted from the checkpoint at {} tokens (sliding window or recurrent state)", n_past);
                        }
                    }
                }
                // Recurrent memory cannot drop a tail: it restarts at the newest checkpoint inside the
                // prefix (llama-server's context checkpoints), else it starts over
                if n_past > 0 && !llama_cpp::llama_memory_seq_rm(mem, 0, n_past as i32, -1) {
                    let ctx = llama.ctx_ptr;
                    n_past = llama
                        .checkpoints
                        .iter()
                        .rev()
                        .find(|c| c.n_tokens > 0 && c.n_tokens as usize <= n_past)
                        .filter(|c| crate::native::context::load_checkpoint(ctx, c))
                        .map_or(0, |c| c.n_tokens as usize);
                    if n_past > 0 && llama_cpp::llama_memory_seq_rm(mem, 0, n_past as i32, -1) {
                        info!("🧠 [Context] Restarted from the checkpoint at {} tokens (recurrent state)", n_past);
                    } else {
                        n_past = 0;
                    }
                }
                if n_past == 0 {
                    llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                }
                llama.checkpoints.retain(|c| c.n_tokens as usize <= n_past);
                cached.truncate(n_past);
                n_past
            };
            report.cache_n = n_past;
            // Why the tokens after the cache are processed (for the context report)
            report.cause = if is_pivot {
                if report.shifted_tokens > 0 { "shift" } else { "append" }
            } else if report.dropped_messages > 0 {
                "history_drop"
            } else if cached_before == 0 {
                "cold"
            } else if n_past + 1 < common_prefix.min(prompt_tokens.len()) {
                report.edit_at = Some(common_prefix);
                "restart"
            } else if common_prefix >= cached_before {
                "append"
            } else {
                report.edit_at = Some(common_prefix);
                "edit"
            };
            let new_tokens = if is_pivot {
                prompt_tokens.clone()
            } else {
                prompt_tokens[n_past..].to_vec()
            };
            cached.extend_from_slice(&new_tokens);
            tracing::debug!(
                "🔍 [KV-Debug] context {} tokens, {} from the cache, {} to process",
                cached.len(),
                n_past,
                new_tokens.len()
            );

            // The request was stopped between two prompt chunks
            let stopped = std::cell::Cell::new(false);
            let decode_from = |llama: &NativeLlama,
                               batch: &mut crate::ffi::llama_cpp::LlamaBatch,
                               tokens: &[i32],
                               start_pos: usize|
             -> bool {
                let cap = if background { BACKGROUND_PROMPT_CHUNK } else { usize::MAX };
                let mut start = 0;
                for len in llama.prompt_chunk_lens(tokens.len(), cap) {
                    let chunk = &tokens[start..(start + len).min(tokens.len())];
                    let chunk_start = start;
                    start += chunk.len();
                    if chunk.is_empty() {
                        break;
                    }
                    if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
                        stopped.set(true);
                        return false;
                    }
                    for (i, token) in chunk.iter().enumerate() {
                        let index = chunk_start + i;
                        *batch.token.add(i) = *token;
                        *batch.pos.add(i) = (start_pos + index) as i32;
                        *batch.n_seq_id.add(i) = 1;
                        *(*batch.seq_id.add(i)).add(0) = 0;
                        *batch.logits.add(i) = i8::from(index + 1 == tokens.len());
                    }
                    batch.n_tokens = chunk.len() as i32;
                    let rc = llama_cpp::llama_decode(llama.ctx_ptr, *batch);
                    if rc != 0 {
                        tracing::warn!(
                            "⚠️ [Llama-Lib] Prefill chunk decode failed with code {} at position {}",
                            rc,
                            start_pos + chunk_start
                        );
                        return false;
                    }
                    // MTP: the drafter sees every target decode, prompt chunks included (llama-server)
                    if let Some(mtp) = llama.mtp.as_ref() {
                        let pos0 = (start_pos + chunk_start) as i32;
                        mtp.seq_rm(pos0);
                        if !mtp.process_run(chunk, pos0, chunk_start + chunk.len() == tokens.len()) {
                            tracing::warn!("🔮 [MTP] The drafter could not read prompt chunk at {pos0}; its guesses may be worse");
                        }
                    }
                }
                true
            };

            // Sliding-window and recurrent (or hybrid) models cannot roll their KV back: checkpoint a
            // few tokens before the prompt end, so the next turn can restart there when its history
            // differs from this one's generation prompt onwards (llama-server does the same)
            let swa = llama_cpp::llama_model_n_swa(llama.model_ptr) > 0
                || llama_cpp::llama_model_is_recurrent(llama.model_ptr)
                || llama_cpp::llama_model_is_hybrid(llama.model_ptr);
            let tail = llama.checkpoint_tail();
            let split = if swa && new_tokens.len() > tail {
                new_tokens.len() - tail
            } else {
                0
            };
            prefill_started = std::time::Instant::now();
            let mut prefilled = decode_from(llama, &mut safe_batch.batch, &new_tokens[..split], n_past);
            if prefilled && split > 0 {
                if let Some(checkpoint) =
                    crate::native::context::take_checkpoint(llama.ctx_ptr, (n_past + split) as i32)
                {
                    crate::native::context::remember_checkpoint(&mut llama.checkpoints, checkpoint);
                }
            }
            prefilled = prefilled
                && decode_from(llama, &mut safe_batch.batch, &new_tokens[split..], n_past + split);
            if stopped.get() {
                // Stopped for a request: the KV keeps the cached prefix, the rest of this prompt goes
                if llama_cpp::llama_memory_seq_rm(mem, 0, n_past as i32, -1) {
                    cached.truncate(n_past);
                    llama.checkpoints.retain(|c| c.n_tokens as usize <= n_past);
                } else {
                    llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                    llama.checkpoints.clear();
                    cached.clear();
                }
                report.stop = "cancel";
                report.n_tokens = cached.len();
                info!("🛑 [NativeStream] Stopped while processing the prompt; {} cached tokens kept", cached.len());
                llama.last_context = Some(report);
                return Ok(cached);
            }
            if !prefilled {
                tracing::warn!("⚠️ [Llama-Lib] Delta prefill failed (KV cache mismatch). Falling back to full prefill from scratch...");
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                llama.checkpoints.clear();
                report.cache_n = 0;
                if !decode_from(llama, &mut safe_batch.batch, &cached, 0) {
                    return Err(anyhow::anyhow!("Fatal: Full prefill fallback failed"));
                }
            }

            let n_cur = cached.len() as i32;
            (cached, n_cur)
        };
        if return_prompt {
            report.prompt = Some(formatted_prompt.clone());
        }
        report.prompt_tokens = n_cur.max(0) as usize;
        report.processed_tokens = report.prompt_tokens.saturating_sub(report.cache_n);
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);
        // Native tool calls: llama.cpp's lazy grammar for the model's call format (asleep while it
        // thinks) and its parser, which turns the output back into text and calls
        let native_apply = native_apply.borrow().clone();
        let native_parser = native_apply.as_ref().filter(|_| parse_tools || parse_answer).and_then(|apply| {
            let tools = llama.chat_tools(dna.chat_template.as_deref())?;
            crate::ffi::chat::ChatParser::new(tools, apply)
                .map_err(|e| tracing::warn!("⚠️ [NativeTools] {}; calls are read as <tool_call> blocks only", e))
                .ok()
        });
        let mut tool_grammar = match (&native_apply, &native_parser) {
            (Some(apply), Some(_)) => crate::ffi::chat::tool_grammar(vocab, apply),
            _ => std::ptr::null_mut(),
        };
        // A strict grammar from the template: the answer's JSON schema and the calls together, or
        // tool_choice "required". Like llama.cpp's common_sampler it applies from the first token,
        // thinking included (the grammar has the thinking block in it), and starts after the
        // generation prompt (`cluaiz_tool_grammar_init` feeds it those tokens)
        let template_strict = !tool_grammar.is_null()
            && native_apply.as_ref().is_some_and(|apply| apply["grammar_lazy"].as_bool() == Some(false));
        let mut grammar_lazy = !template_strict;
        if template_strict && template_schema.is_some() {
            if native_tools.is_some() {
                info!("📐 [NativeStream] Answer constrained by a JSON schema grammar, in one grammar with the tool calls (llama.cpp)");
            } else {
                info!("📐 [NativeStream] Answer constrained by a JSON schema grammar from the template, thinking block included (llama.cpp)");
            }
        } else if tools_callable && !tool_grammar.is_null() && matches!(req_output, Some(cluaiz_core::OutputConstraint::JsonSchema(_))) {
            info!("📐 [NativeStream] JSON schema with callable tools: the answer is not constrained, so calls stay possible");
        } else if !tool_grammar.is_null() && req_output.is_some() {
            tracing::warn!("⚠️ [NativeStream] This template's tool-call grammar did not take the request's grammar or JSON schema; the answer is not constrained");
        }
        if template_strict && tool_choice.as_deref() == Some("required") {
            info!("🧰 [NativeTools] tool_choice required: the answer must be a call");
        }
        if tool_choice.as_deref() == Some("required") && native_tools.is_some() && native_apply.is_none() {
            tracing::warn!("⚠️ [NativeTools] tool_choice required is not enforced: the template does not render tools natively");
        }
        // Without a tool grammar, the request's own grammar or JSON schema shapes the answer
        // (llama-server's `grammar` / `response_format`); it too waits until thinking is done
        if tool_grammar.is_null() {
            let gbnf = match &req_output {
                Some(cluaiz_core::OutputConstraint::Gbnf(g)) => Some(g.clone()),
                Some(cluaiz_core::OutputConstraint::JsonSchema(schema)) => crate::ffi::chat::json_schema_grammar(schema)
                    .map_err(|e| tracing::warn!("⚠️ [NativeStream] JSON schema not supported ({}); answer not constrained", e))
                    .ok(),
                None => None,
            };
            if let Some(gbnf) = gbnf {
                let spec = serde_json::json!({ "grammar": gbnf, "grammar_lazy": false, "triggers": [] });
                tool_grammar = crate::ffi::chat::tool_grammar(vocab, &spec);
                grammar_lazy = false;
                if tool_grammar.is_null() {
                    tracing::warn!("⚠️ [NativeStream] llama.cpp could not parse the grammar; answer not constrained");
                } else {
                    info!("📐 [NativeStream] Answer constrained by a {} grammar", if matches!(req_output, Some(cluaiz_core::OutputConstraint::Gbnf(_))) { "GBNF" } else { "JSON schema" });
                }
            }
        }
        let mut additional_stops: Vec<String> = native_apply
            .as_ref()
            .and_then(|apply| apply["additional_stops"].as_array())
            .map(|stops| stops.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        additional_stops.extend(req_stops.iter().cloned());
        if let Some(apply) = &native_apply {
            info!(
                "🧰 [NativeTools] Format {} | grammar {} (lazy {}, {} triggers)",
                apply["format"].as_str().unwrap_or("?"),
                if tool_grammar.is_null() { "none" } else { "on" },
                apply["grammar_lazy"].as_bool().unwrap_or(false),
                apply["triggers"].as_array().map_or(0, Vec::len)
            );
        }
        // The request's own grammar (a card, `grammar`, `response_format`) stays out of the chain:
        // `sample_token` checks only the picked token against it (llama.cpp's common_sampler), and
        // this guard frees it. A tool-call grammar stays first in the chain, which owns it, as
        // before (the per-token check was not measured on tool calls).
        let checked_grammar = if grammar_lazy { std::ptr::null_mut() } else { tool_grammar };
        let _grammar_owner = SafeSampler { sampler: checked_grammar };
        let sampler_chain_raw = crate::native::sampler::build_sampler_chain(
            dna,
            &full_prompt_tokens,
            req_samplers.as_ref(),
            &req_logit_bias,
            n_vocab,
            if grammar_lazy { tool_grammar } else { std::ptr::null_mut() },
        )?;
        let mut safe_sampler = SafeSampler {
            sampler: sampler_chain_raw,
        };

        let mut history: Vec<i32> = full_prompt_tokens;
        let mut utf8_buffer = Vec::new();

        let mut n_gen = 0;
        cluaiz_core::live::start();

        // Room for thinking = context left after the prompt minus the answer's reserve, so whatever budget
        // was chosen, the answer still fits; Low/Medium/High are shares of it (settings), Auto is set
        // by the last measured decode speed, at most High
        let room = n_ctx.saturating_sub(n_cur.max(0) as usize + gen_reserve);
        let presets = cluaiz_core::Settings::current()
            .optimization
            .thinking_presets;
        let limit = thinking_budget.limit(room, &presets, llama.decode_per_second);
        let max_think_tokens = limit.tokens;
        // The sampler's hard limit; between the budget and it, thinking closes at the next line end
        let hard_think_tokens = limit.tokens + limit.grace;
        if thinking_budget == cluaiz_core::ThinkingBudget::Auto && enable_thinking {
            match llama.decode_per_second {
                Some(speed) => info!(
                    "🧠 [NativeStream] Auto thinking ceiling {} tokens ({:.1} t/s for up to {:.0} s, room {})",
                    max_think_tokens, speed, cluaiz_core::AUTO_THINK_SECONDS, room
                ),
                None => info!(
                    "🧠 [NativeStream] Auto thinking ceiling {} tokens (High's share of room {}; no speed measured yet)",
                    max_think_tokens, room
                ),
            }
        }
        if limit.capped {
            info!(
                "🧠 [NativeStream] Thinking budget {:?} lowered to {} tokens so the answer fits the context",
                thinking_budget, limit.tokens
            );
        }
        let mut ended_by = "model";
        // Thinking still open at the model's end was closed for it (once per generation)
        let mut closed_open_thinking = false;
        let mut thought = false;
        // What is written to close thinking early: the request's or the settings' budget message, else
        // the managed `chat.thinking_close` prompt (Qwen's early-stopping text), then the end tag, so the
        // model answers from what it thought instead of stopping mid-thought
        let close_text = match &req_budget_message {
            Some(message) => format!("{}{}", message, think_end_tag),
            None => format!("\n{}\n{}", cluaiz_core::prompts::text("chat.thinking_close").trim(), think_end_tag),
        };
        let (root, reasoning) = crate::ffi::reasoning::ReasoningHandle::wrap(
            safe_sampler.sampler, vocab, (&think_start_tag, &think_end_tag, &close_text),
            hard_think_tokens, prompt_opens_thinking,
        )?;
        // The last thinking token ended a line (set per token below)
        let mut think_line_end = false;
        safe_sampler.sampler = root;
        // The grammar sees only what the model writes outside thinking (a strict template grammar: everything)
        let mut grammar_active = false;
        let mut set_grammar = |active: bool, replay: &str| {
            if !tool_grammar.is_null() && active != grammar_active {
                crate::ffi::chat::set_tool_grammar_active(tool_grammar, vocab, active, replay);
                grammar_active = active;
            }
        };
        // The prompt's own thinking end tag (thinking off) is not output: nothing to replay
        set_grammar(template_strict || !prompt_opens_thinking, "");
        // Everything generated (thinking included) for the parser, the answer part, and what of
        // the parsed answer was sent
        let mut generated = String::new();
        let mut answer_raw = String::new();
        // Answer text held back while it could be the start of a request `stop` string
        let mut stop_hold = String::new();
        let mut content_stream = cluaiz_core::tool_calls::ContentStream::default();
        let skip_requested = || skip_reasoning
            .is_some_and(|signal| signal.load(Ordering::Acquire));

        // Sample the first token from the prompt prefill logits
        if prompt_opens_thinking {
            in_think_block = true;
            if !suppress_thinking {
                let _ = callback(think_start_tag.clone());
            }
        }

        // MTP (not with media in the prompt: the drafter did not see it). `check`: guesses the target
        // decoded in one batch and the index of the next one to compare; `row`: the logits row the
        // next token is sampled from (-1: the last, no guesses pending)
        let mut mtp = llama.mtp.as_ref().filter(|_| !is_media && !is_pivot);
        if let Some(m) = mtp {
            m.begin(&history);
        }
        let mtp_before = mtp.map(|m| m.totals());
        let mut check: Option<(Vec<i32>, usize)> = None;
        let mut row: i32 = -1;
        // With MTP the prompt numbers are the wall clock of the prompt work: the target's decodes and
        // the drafter reading every chunk (its cost on long prompts), up to the moment the last chunk
        // has finished on the device. llama.cpp's counters see only the target's decodes, and only
        // after a synchronize, so they missed chunks (0.0 t/s in a split placement). The generation
        // is timed here too (see `NativeLlama::mtp_timings`)
        let prompt_timings = mtp.map(|_| {
            llama_cpp::llama_synchronize(llama.ctx_ptr);
            let ms = prefill_started.elapsed().as_secs_f64() * 1000.0;
            let n = report.processed_tokens as i32;
            crate::native::core::GenerationTimings {
                prompt_n: n,
                prompt_ms: ms,
                prompt_per_second: if ms > 0.0 { n as f64 * 1000.0 / ms } else { 0.0 },
                ..Default::default()
            }
        });
        let generation_started = std::time::Instant::now();
        let mut tokens_out: i32 = 0;
        // MTP guard (Auto mode, `NativeLlama::mtp_guard`): the first GUARD_PROBE steps of an answer
        // decode one token each without guesses and give this answer's plain speed (the drafter still
        // reads each token, so plain is measured a little slow: the guard leans to keeping MTP).
        // Drafting steps are then timed; every GUARD_JUDGE tokens from them, drafting slower than plain
        // stops MTP for the rest of the answer. A limit on the kept share would not do: run 23 found the
        // break-even between about 40% kept (MoE, all on the GPU) and 85% (MoE, experts in RAM).
        let guard = mtp.is_some() && llama.mtp_guard;
        let mut guard_plain = GuardSpan::default();
        let mut guard_draft = GuardSpan::default();
        let mut guard_probed = 0usize;
        let mut guard_step: Option<(std::time::Instant, i32, bool)> = None;
        let mut guard_stopped = false;
        let mut guard_rates: (Option<f64>, Option<f64>) = (None, None);

        reasoning.poll(skip_requested(), false, &mut ended_by);
        let mut next_token_id =
            crate::native::sampler::sample_token(safe_sampler.sampler, checked_grammar, llama.ctx_ptr, n_vocab);
        tracing::debug!(
            "🎲 [NativeStream] Initial sampled token_id: {}",
            next_token_id
        );

        loop {
            if n_gen >= max_tokens as i32 {
                report.stop = "limit";
                break;
            }
            // Checked on every token: while a tool call is being written nothing reaches the
            // callback, so a stop asked for then would wait for the whole call
            if llama.interrupt_signal.load(Ordering::SeqCst)
                || cancel.unwrap_or(&cluaiz_core::GLOBAL_CANCEL_SIGNAL).load(Ordering::SeqCst)
            {
                report.stop = "cancel";
                break;
            }

            // Upstream standard: check if token is end of generation
            if llama_cpp::llama_vocab_is_eog(vocab, next_token_id) {
                // The model ended while its thinking was still open: its answer or tool call is
                // inside the thinking, where no parser takes it and the user sees nothing. Thinking
                // is closed for it once, as the thinking budget closes it (llama.cpp's reasoning
                // sampler writes the template's end tag), and the model goes on outside it.
                if in_think_block && !think_end_tag.is_empty() && !closed_open_thinking && reasoning.force() {
                    closed_open_thinking = true;
                    ended_by = "closed";
                    info!("🧠 [NativeStream] The model ended inside its thinking; thinking closed so it answers outside it");
                    next_token_id = crate::native::sampler::sample_token_at(safe_sampler.sampler, checked_grammar, llama.ctx_ptr, n_vocab, row);
                    continue;
                }
                tracing::info!(
                    "🛑 [NativeStream] EOG reached for token={}. Gracefully terminating generation.",
                    next_token_id
                );
                break;
            }

            // sample_token already accepted this token into the chain (penalties see it once)
            history.push(next_token_id);
            tokens_out += 1;

            // Convert token to UTF-8 piece (special = true ensures reasoning control tokens like <think> and <|channel> are emitted)
            let mut buf = [0u8; 256];
            let n_bytes = llama_cpp::llama_token_to_piece(
                vocab,
                next_token_id,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as i32,
                0,
                true,
            );

            if n_bytes > 0 {
                utf8_buffer.extend_from_slice(&buf[..n_bytes as usize]);
                let mut piece = String::new();
                match std::str::from_utf8(&utf8_buffer) {
                    Ok(s) => {
                        piece = s.to_string();
                        utf8_buffer.clear();
                    }
                    Err(e) => {
                        let valid_len = e.valid_up_to();
                        if valid_len > 0 {
                            piece = String::from_utf8_lossy(&utf8_buffer[..valid_len]).to_string();
                            utf8_buffer.drain(..valid_len);
                        }
                        if let Some(error_len) = e.error_len() {
                            utf8_buffer.drain(..error_len);
                        }
                    }
                }

                if !piece.is_empty() {
                    let emit_str = piece.as_str();
                    let was_thinking = in_think_block;

                    // Dynamic thinking tag state tracking over the recent text (tags may span tokens)
                    tag_window.push_str(emit_str);
                    if !think_start_tag.is_empty() && tag_window.contains(&think_start_tag) {
                        in_think_block = true;
                        tag_window.clear();
                    }
                    if !think_end_tag.is_empty() && tag_window.contains(&think_end_tag) {
                        in_think_block = false;
                        tag_window.clear();
                    }
                    if tag_window.len() > tag_window_len {
                        let mut cut = tag_window.len() - tag_window_len;
                        while !tag_window.is_char_boundary(cut) {
                            cut += 1;
                        }
                        tag_window.drain(..cut);
                    }

                    think_line_end = in_think_block && emit_str.contains('\n');

                    // Thinking just ended: the grammar wakes up and sees the end tag (llama-server)
                    // (a strict grammar starts at the answer itself: nothing to replay)
                    if !template_strict {
                        set_grammar(!in_think_block, if grammar_lazy { &think_end_tag } else { "" });
                    }
                    generated.push_str(emit_str);
                    // A start tag over several tokens (Gemma 4: "<|channel>" + "thought"): its first
                    // tokens went to the answer parser, which sends nothing of them. The reader of the
                    // stream needs the whole tag, or the thinking looks like the answer.
                    let mut opened = None;
                    if in_think_block && !was_thinking && native_parser.is_some() {
                        let held = (1..think_start_tag.len())
                            .rev()
                            .filter(|n| think_start_tag.is_char_boundary(*n))
                            .find(|n| answer_raw.ends_with(&think_start_tag[..*n]))
                            .unwrap_or(0);
                        answer_raw.truncate(answer_raw.len() - held);
                        let after = emit_str.strip_prefix(&think_start_tag[held..]).unwrap_or("");
                        opened = Some(format!("{think_start_tag}{after}"));
                    }
                    if native_parser.is_some() && !in_think_block && !was_thinking {
                        // The answer goes out as llama.cpp's parser reads it: text only, the call
                        // markup is held back and sent in the Engine's form when generation ends
                        answer_raw.push_str(emit_str);
                        if let Some(stop) = additional_stops.iter().find(|stop| answer_raw.contains(stop.as_str())) {
                            let cut = generated.len() - (answer_raw.len() - answer_raw.find(stop.as_str()).unwrap_or(answer_raw.len()));
                            generated.truncate(cut);
                            report.stop = "stop";
                            break;
                        }
                        let parsed = native_parser.as_ref().and_then(|parser| parser.parse(&generated, true).ok());
                        // The call being written is not sent yet: a terminal shows it as progress
                        if let Some(call) = parsed.as_ref().and_then(|p| p["tool_calls"].as_array()).and_then(|calls| calls.last()) {
                            cluaiz_core::live::set_call(call["name"].as_str().unwrap_or_default(), call["arguments"].as_str().unwrap_or_default());
                        }
                        if let Some(delta) = parsed.as_ref().and_then(|p| p["content"].as_str()).and_then(|c| content_stream.delta(if native_tools.is_some() { cluaiz_core::tool_calls::before_bare_call(c) } else { c })) {
                            if !callback(delta) {
                                report.stop = "cancel";
                                break;
                            }
                        }
                    } else if in_think_block && suppress_thinking {
                        // hidden thinking: not sent, still counted below
                    } else if !emit_str.is_empty() {
                        tracing::debug!(
                            "📤 [NativeStream] Emitting token_id {}: {:?}",
                            next_token_id,
                            emit_str
                        );
                        let text = opened.unwrap_or_else(|| emit_str.to_string());
                        if !req_stops.is_empty() && !in_think_block && !was_thinking {
                            // Request `stop` strings: send nothing that could begin one; at a match
                            // send the text before it and end the answer (finish_reason "stop")
                            stop_hold.push_str(&text);
                            match stop_match(&stop_hold, &req_stops) {
                                StopScan::Found(at) => {
                                    if at > 0 && !callback(stop_hold[..at].to_string()) {
                                        report.stop = "cancel";
                                        break;
                                    }
                                    stop_hold.clear();
                                    report.stop = "stop";
                                    break;
                                }
                                StopScan::Send(upto) => {
                                    if upto > 0 {
                                        let out: String = stop_hold.drain(..upto).collect();
                                        if !callback(out) {
                                            report.stop = "cancel";
                                            break;
                                        }
                                    }
                                }
                            }
                        } else if !callback(text) {
                            report.stop = "cancel";
                            break;
                        }
                    }
                }
            }

            cluaiz_core::live::token();
            if in_think_block {
                think_tokens_count += 1;
                thought = true;
            } else {
                n_gen += 1;
            }
            // Context full. An answer asked to be longer than the room left (max_tokens) continues with
            // llama-server's context shift (keep BOS, discard the oldest of the rest) when the mode and
            // the memory allow it. Without max_tokens it stops here like llama-server's default
            // (finish_reason "length"): an answer longer than the context would push out the question
            // itself and could not be sent back in the next turn.
            if n_cur as usize + 1 >= n_ctx {
                let n_keep = usize::from(add_bos);
                let n_left = (n_cur as usize).saturating_sub(n_keep);
                let longer_answer_asked = max_tokens < n_ctx;
                match cluaiz_core::shift_discard(shifting, n_ctx, n_left)
                    .filter(|_| can_shift && longer_answer_asked)
                {
                    Some(n_discard) => {
                        crate::native::context::shift_context(
                            llama.ctx_ptr,
                            n_cur,
                            n_keep as i32,
                            n_discard as i32,
                        );
                        llama.checkpoints.clear();
                        // The draft context's positions no longer match: no guesses for the rest
                        if let Some(m) = mtp.take() {
                            m.seq_rm(0);
                        }
                        history.drain(n_keep..n_keep + n_discard);
                        n_cur -= n_discard as i32;
                        report.shifted_tokens += n_discard;
                        report.truncated = true;
                        tracing::warn!(
                            "🌊 [Context] Context full ({} tokens): shifted out {} old tokens and continued ({:?})",
                            n_ctx, n_discard, shifting
                        );
                    }
                    None => {
                        report.truncated = true;
                        report.stop = "limit";
                        tracing::warn!(
                            "🛑 [NativeStream] Context window full (n_cur={}, n_ctx={}). Stopping ({}).",
                            n_cur,
                            n_ctx,
                            if !can_shift {
                                "this model's memory cannot shift"
                            } else if !longer_answer_asked {
                                "no max_tokens beyond the context"
                            } else {
                                "context shifting is Off"
                            }
                        );
                        break;
                    }
                }
            }

            // MTP: the token just handled was the next guess and the target agrees: it is in the KV
            // already, and the next token's logits are the next row of the same decode
            let mut in_kv = false;
            if let Some((draft, next)) = check.as_mut() {
                let j = *next;
                if j < draft.len() && next_token_id == draft[j] {
                    n_cur += 1;
                    *next += 1;
                    row = *next as i32;
                    in_kv = true;
                } else {
                    // Refused at j (or j == all guesses: every one agreed and this token is the extra
                    // one): the KV keeps the agreed guesses, the rest goes
                    let refused = j < draft.len();
                    check = None;
                    if let Some(m) = mtp {
                        m.accept(j);
                        m.seq_rm(n_cur);
                    }
                    if refused && !llama_cpp::llama_memory_seq_rm(mem, 0, n_cur, -1) {
                        tracing::error!("❌ [MTP] The target could not drop refused guesses at {n_cur}");
                        report.stop = "error";
                        break;
                    }
                }
            }

            if !in_kv {
                // MTP guard: the step that ends here goes into its span; judge when enough was drafted
                let mut probing = false;
                if guard && mtp.is_some() {
                    let now = std::time::Instant::now();
                    if let Some((started, tokens0, drafting)) = guard_step.take() {
                        let span = if drafting { &mut guard_draft } else { &mut guard_plain };
                        span.tokens += (tokens_out - tokens0).max(0) as u32;
                        span.secs += now.duration_since(started).as_secs_f64();
                    }
                    if guard_draft.tokens >= GUARD_JUDGE && guard_plain.secs > 0.0 && guard_draft.secs > 0.0 {
                        let (plain, drafting) = (guard_plain.rate(), guard_draft.rate());
                        guard_rates = (Some(plain), Some(drafting));
                        if drafting < plain {
                            if let Some(m) = mtp.take() {
                                m.seq_rm(n_cur);
                            }
                            guard_stopped = true;
                            info!("🔮 [MTP] Guard: {drafting:.1} t/s with guesses, {plain:.1} t/s without; no more guesses in this answer");
                        }
                        guard_draft = GuardSpan::default();
                    }
                    if mtp.is_some() {
                        probing = guard_probed < GUARD_PROBE;
                        guard_probed += usize::from(probing);
                        guard_step = Some((now, tokens_out, !probing));
                    }
                }
                // Guesses for after this token, as many as fit the context and the answer left
                let draft = match mtp {
                    Some(m) => {
                        let room = n_ctx as i32 - n_cur - 3;
                        let left = max_tokens as i32 - n_gen;
                        let n_max = if probing { 0 } else { m.n_max().min(room).min(left) };
                        let draft = if n_max > 0 {
                            m.draft(n_cur, next_token_id, &history[..history.len().saturating_sub(1)], n_max)
                        } else {
                            Vec::new()
                        };
                        // What the drafter wrote from this token on is decoded again with the target's rows
                        m.seq_rm(n_cur);
                        draft
                    }
                    None => Vec::new(),
                };

                // The token, then the guesses, each with its logits: one decode checks them all
                let batch_tokens: Vec<i32> = std::iter::once(next_token_id).chain(draft.iter().copied()).collect();
                for (i, token) in batch_tokens.iter().enumerate() {
                    *safe_batch.batch.token.add(i) = *token;
                    *safe_batch.batch.pos.add(i) = n_cur + i as i32;
                    *safe_batch.batch.n_seq_id.add(i) = 1;
                    *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                    *safe_batch.batch.logits.add(i) = 1;
                }
                safe_batch.batch.n_tokens = batch_tokens.len() as i32;

                if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                    if is_media {
                        anyhow::bail!("Media generation decode failed at position {n_cur}");
                    }
                    tracing::error!(
                        "❌ [NativeStream] llama_decode failed at position {}",
                        n_cur
                    );
                    report.stop = "error";
                    break;
                }
                if let Some(m) = mtp {
                    if !m.process_run_all(&batch_tokens, n_cur) {
                        tracing::warn!("🔮 [MTP] The drafter could not read the target's decode; no more guesses in this answer");
                        mtp = None;
                    }
                }

                n_cur += 1;
                if draft.is_empty() {
                    row = -1;
                } else {
                    check = Some((draft, 0));
                    row = 0;
                }
            }

            // Sample next token. A spent budget closes thinking at the end of a line (the hard limit
            // budget + grace is the sampler's own backstop).
            let line_done = think_line_end && limit.grace > 0 && think_tokens_count >= max_think_tokens;
            reasoning.poll(skip_requested(), line_done, &mut ended_by);
            think_line_end = false;
            next_token_id =
                crate::native::sampler::sample_token_at(safe_sampler.sampler, checked_grammar, llama.ctx_ptr, n_vocab, row);
        }

        // Text held for a `stop` string that never came is part of the answer
        if !stop_hold.is_empty() && report.stop != "stop" && report.stop != "cancel" {
            callback(std::mem::take(&mut stop_hold));
        }

        // Guesses left unchecked when generation stopped leave the KV
        if check.is_some() {
            if let Some(m) = mtp {
                m.seq_rm(n_cur);
            }
            llama_cpp::llama_memory_seq_rm(mem, 0, n_cur, -1);
        }
        if let (Some(m), Some((drafted0, accepted0))) = (llama.mtp.as_ref(), mtp_before) {
            let (drafted, accepted) = m.totals();
            let (drafted, accepted) = (drafted - drafted0, accepted - accepted0);
            if drafted > 0 {
                info!("🔮 [MTP] {accepted} of {drafted} guessed tokens accepted ({:.0}%)", 100.0 * accepted as f64 / drafted as f64);
                report.mtp = Some(crate::native::core::MtpReport {
                    drafted,
                    accepted,
                    stopped: guard_stopped,
                    plain_tps: guard_rates.0,
                    drafting_tps: guard_rates.1,
                });
                if let Some(prompt) = prompt_timings {
                    let ms = generation_started.elapsed().as_secs_f64() * 1000.0;
                    llama.mtp_timings = Some(crate::native::core::GenerationTimings {
                        predicted_n: tokens_out,
                        predicted_ms: ms,
                        predicted_per_second: if ms > 0.0 { tokens_out as f64 * 1000.0 / ms } else { 0.0 },
                        ..prompt
                    });
                }
            }
        }

        // Native tool calls: the rest of the answer, then each call as a <tool_call> block
        if let Some(parser) = &native_parser {
            if report.stop != "cancel" {
                match parser.parse(&generated, false) {
                    Ok(parsed) => {
                        let content = parsed["content"].as_str().unwrap_or_default();
                        let mut calls = parsed["tool_calls"].as_array().cloned().unwrap_or_default();
                        // Stopped by max_tokens or a full context: llama.cpp's final parse is lenient,
                        // so a call cut in the middle comes back with half its arguments (run 70:
                        // `write_file` ran with a cut payload). llama-server returns such calls with
                        // finish_reason "length" and runs nothing (`server-task.cpp`,
                        // `to_json_oaicompat_chat`); here a block sent on is run, so none is sent and
                        // the answer is the text before the call
                        let limited = report.stop == "limit";
                        if limited && !calls.is_empty() {
                            tracing::warn!("🧰 [NativeTools] {} call(s) cut by the token limit: not sent, nothing runs", calls.len());
                            calls.clear();
                        }
                        let bare = (!limited && calls.is_empty() && native_tools.is_some()).then(|| cluaiz_core::tool_calls::call_without_open(content)).flatten();
                        let text = bare.as_ref().map_or(content, |(before, _, _)| before.as_str());
                        if let Some(delta) = content_stream.delta(text) {
                            let _ = callback(delta);
                        }
                        if let Some((_, name, arguments)) = &bare {
                            info!("🧰 [NativeTools] A call without its opening tag: {name}");
                            let _ = callback(format!("\n{}", cluaiz_core::tool_calls::canonical_call(name, arguments)));
                        }
                        for call in &calls {
                            let name = call["name"].as_str().unwrap_or_default();
                            let block = cluaiz_core::tool_calls::canonical_call(name, call["arguments"].as_str().unwrap_or("{}"));
                            let _ = callback(format!("\n{block}"));
                        }
                        info!("🧰 [NativeTools] Parsed {} call(s): {:?}", calls.len(),
                            calls.iter().filter_map(|c| c["name"].as_str()).collect::<Vec<_>>());
                    }
                    Err(e) => {
                        // Not readable in the model's format: send what was held back as it was
                        tracing::warn!("⚠️ [NativeTools] Output not parsed ({}); sent as written", e);
                        if let Some(rest) = answer_raw.strip_prefix(content_stream.sent()) {
                            let _ = callback(rest.to_string());
                        } else if content_stream.sent().is_empty() {
                            let _ = callback(answer_raw.clone());
                        }
                    }
                }
            }
        }

        llama.last_thinking = thought.then(|| crate::native::core::ThinkingReport {
            effort: thinking_budget,
            budget_tokens: max_think_tokens,
            capped: limit.capped,
            used_tokens: think_tokens_count,
            ended_by,
        });

        let t = llama.timings();
        // Remembered for Auto effort; very short answers give a noisy speed and are skipped
        if t.predicted_n >= MIN_TOKENS_FOR_SPEED && t.predicted_per_second > 0.0 {
            llama.decode_per_second = Some(t.predicted_per_second);
        }
        info!(
            "⏱️ [NativeStream] llama.cpp timings: prompt {} tokens in {:.0} ms ({:.2} t/s) | generation {} tokens in {:.0} ms ({:.2} t/s)",
            t.prompt_n, t.prompt_ms, t.prompt_per_second, t.predicted_n, t.predicted_ms, t.predicted_per_second
        );

        history.truncate(n_cur as usize);
        report.n_tokens = n_cur.max(0) as usize;
        info!(
            "🧠 [Context] prompt {} tokens ({} from the cache) | context {} of {} | stop {}{}",
            report.prompt_tokens,
            report.cache_n,
            report.n_tokens,
            n_ctx,
            report.stop,
            if report.truncated { " | truncated" } else { "" }
        );
        llama.last_context = Some(report);
        Ok(history)
    }
}

/// Smallest shared chunk `--cache-reuse` moves instead of reprocessing (llama-server's preset value)
const CACHE_REUSE_MIN_CHUNK: usize = 256;

/// `render_prompt`, and when the template has no place for the `tool` role (it drops tool results
/// without an error), again with the results as `<tool_response>` user turns
fn render_inline(
    llama: &NativeLlama,
    dna: &StructuralDNA,
    messages: &[(String, String)],
    enable_thinking: bool,
    is_pivot: bool,
    think_end_tag: &str,
) -> anyhow::Result<String> {
    let rendered = render_prompt(llama, dna, messages, enable_thinking, is_pivot, think_end_tag)?;
    let dropped_tool = cluaiz_core::tool_calls::missing_messages(&rendered, messages)
        .into_iter()
        .any(|i| messages[i].0 == "tool");
    if !dropped_tool {
        return Ok(rendered);
    }
    tracing::info!("🧰 [NativeTemplate] The template drops tool results; they go in as <tool_response> user turns");
    let as_user = cluaiz_core::tool_calls::tool_results_as_user(messages);
    render_prompt(llama, dna, &as_user, enable_thinking, is_pivot, think_end_tag)
}

/// `messages` through the model's chat template (llama.cpp's common template engine). A continuation
/// (pivot) first closes an open thinking block.
fn render_prompt(
    llama: &NativeLlama,
    dna: &StructuralDNA,
    messages: &[(String, String)],
    enable_thinking: bool,
    is_pivot: bool,
    think_end_tag: &str,
) -> anyhow::Result<String> {
    let msg_refs: Vec<(&str, &str)> = messages
        .iter()
        .map(|(r, c)| (r.as_str(), c.as_str()))
        .collect();
    match crate::native::templater::apply_chat_template(
        llama.model_ptr,
        llama.chat_templates_for(dna.chat_template.as_deref()),
        dna.chat_template.as_deref(),
        &msg_refs,
        true,
        enable_thinking,
    ) {
        Ok(rendered) if !rendered.trim().is_empty() => {
            if is_pivot && !think_end_tag.is_empty() {
                Ok(format!("\n{}\n{}", think_end_tag, rendered))
            } else {
                Ok(rendered)
            }
        }
        Ok(_) => Err(anyhow::anyhow!(
            "llama_chat_apply_template returned empty prompt"
        )),
        Err(e) => {
            tracing::error!("❌ [NativeTemplate] llama.cpp chat template error: {}", e);
            Err(e)
        }
    }
}

/// Plain steps the MTP guard times at the start of an answer, one token each
const GUARD_PROBE: usize = 8;
/// Tokens from drafting steps between two guard checks
const GUARD_JUDGE: u32 = 32;

/// Tokens and seconds of one kind of decode step (MTP guard)
#[derive(Default)]
struct GuardSpan {
    tokens: u32,
    secs: f64,
}

impl GuardSpan {
    fn rate(&self) -> f64 {
        if self.secs > 0.0 { self.tokens as f64 / self.secs } else { 0.0 }
    }
}

/// Where a request `stop` string stands in the held answer text
#[derive(Debug, PartialEq)]
enum StopScan {
    /// A stop string starts at this byte: the answer ends before it
    Found(usize),
    /// No stop string yet: the first this many bytes can be sent; the rest could begin one
    Send(usize),
}

fn stop_match(held: &str, stops: &[String]) -> StopScan {
    if let Some(at) = stops.iter().filter_map(|stop| held.find(stop.as_str())).min() {
        return StopScan::Found(at);
    }
    // The longest end of `held` that is the beginning of a stop string stays held
    let keep = stops
        .iter()
        .flat_map(|stop| (1..stop.len()).filter(|n| stop.is_char_boundary(*n)).map(move |n| &stop[..n]))
        .filter(|prefix| held.ends_with(prefix))
        .map(str::len)
        .max()
        .unwrap_or(0);
    StopScan::Send(held.len() - keep)
}

#[cfg(test)]
mod stop_tests {
    use super::{stop_match, StopScan};

    fn stops(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn stop_strings_end_the_answer_before_them() {
        assert_eq!(stop_match("1
2
3
4
5", &stops(&["5"])), StopScan::Found(8));
        assert_eq!(stop_match("abc", &stops(&["x"])), StopScan::Send(3));
        // "EN" could begin "END": held back
        assert_eq!(stop_match("the EN", &stops(&["END"])), StopScan::Send(4));
        assert_eq!(stop_match("the END here", &stops(&["END", "the"])), StopScan::Found(0));
        // multi-byte text is cut only at character boundaries
        assert_eq!(stop_match("नमस्", &stops(&["स्ते"])), StopScan::Send("नम".len()));
    }
}
