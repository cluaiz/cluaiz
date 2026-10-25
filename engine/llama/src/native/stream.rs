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
        let (req_samplers, req_think_mode, req_reasoning_effort, req_output, req_budget_message, req_kv_reuse, req_tools, keep_prefix) =
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
                    // same prompt for a job that wants plain text, like an idle-time card)
                    let tools = envelope
                        .get("tools")
                        .filter(|t| t.as_array().is_some_and(|a| !a.is_empty()))
                        .cloned()
                        .map(|tools| (tools, envelope.get("parse_tools").and_then(|p| p.as_bool()) != Some(false)));
                    // Thinking off without changing the start of the prompt (a job that reuses a
                    // chat's KV): see below
                    let keep_prefix = envelope.get("thinking_off_keeps_prefix").and_then(|k| k.as_bool()) == Some(true);
                    return_prompt = envelope.get("return_prompt").and_then(|k| k.as_bool()) == Some(true);
                    background = envelope.get("background").and_then(|k| k.as_bool()) == Some(true);
                    (
                        samplers,
                        think_mode,
                        reasoning_effort,
                        grammar,
                        budget_message,
                        kv_reuse,
                        tools,
                        keep_prefix,
                    )
                } else {
                    (None, None, None, None, None, None, None, false)
                }
            } else {
                (None, None, None, None, None, None, None, false)
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
        // (sliding-window / recurrent caches) and for multimodal prompts
        let can_shift = !is_media && llama_cpp::llama_memory_can_shift(mem);
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
        // Media prompts and continuations keep the inline form.
        let (native_tools, parse_tools) = match req_tools.filter(|_| !is_media && !is_pivot) {
            Some((tools, parse)) => (Some(tools), parse),
            None => (None, false),
        };
        if native_tools.is_some() && llama.chat_tools_for(dna.chat_template.as_deref()).is_none() {
            tracing::warn!("⚠️ [NativeTools] llama.cpp could not parse the template for tool use; tools go inline");
        }
        // What llama.cpp returned for the last native render (grammar, triggers, parser)
        let native_apply = std::cell::RefCell::new(None::<serde_json::Value>);
        let render = |llama: &NativeLlama, messages: &[(String, String)]| {
            if let Some(tools) = &native_tools {
                if let Some(chat_tools) = llama.chat_tools(dna.chat_template.as_deref()) {
                    let request = serde_json::json!({
                        "messages": cluaiz_core::tool_calls::openai_messages(messages),
                        "tools": tools,
                        "enable_thinking": enable_thinking,
                    });
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
            render_inline(llama, dna, messages, enable_thinking, is_pivot, &think_end_tag)
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
            while !is_pivot {
                let overflow = (prompt_tokens.len() + fit_reserve).saturating_sub(n_ctx);
                let Some(target) = (overflow > 0)
                    .then(|| cluaiz_core::tokens_to_free(shifting, n_ctx, overflow, can_shift))
                    .flatten()
                else {
                    break;
                };
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
                for (_, n) in &mut sizes {
                    *n += per_message;
                }
                let (n_drop, freed) = cluaiz_core::oldest_turns_to_drop(&sizes, target);
                if n_drop == 0 {
                    break;
                }
                let start = usize::from(structured_messages[0].0.eq_ignore_ascii_case("system"));
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
                let mut step = llama.prompt_chunk_len(tokens.len());
                if background {
                    step = step.min(BACKGROUND_PROMPT_CHUNK);
                }
                for (chunk_idx, chunk) in tokens.chunks(step).enumerate() {
                    if cancel.is_some_and(|c| c.load(Ordering::SeqCst)) {
                        stopped.set(true);
                        return false;
                    }
                    for (i, token) in chunk.iter().enumerate() {
                        let index = chunk_idx * step + i;
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
                            start_pos + chunk_idx * step
                        );
                        return false;
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
            let split = if swa && new_tokens.len() > crate::native::context::CHECKPOINT_TAIL {
                new_tokens.len() - crate::native::context::CHECKPOINT_TAIL
            } else {
                0
            };
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
        let native_parser = native_apply.as_ref().filter(|_| parse_tools).and_then(|apply| {
            crate::ffi::chat::ChatParser::new(apply)
                .map_err(|e| tracing::warn!("⚠️ [NativeTools] {}; calls are read as <tool_call> blocks only", e))
                .ok()
        });
        let mut tool_grammar = match (&native_apply, &native_parser) {
            (Some(apply), Some(_)) => crate::ffi::chat::tool_grammar(vocab, apply),
            _ => std::ptr::null_mut(),
        };
        // Without a tool grammar, the request's own grammar or JSON schema shapes the answer
        // (llama-server's `grammar` / `response_format`); it too waits until thinking is done
        let mut grammar_lazy = true;
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
        let additional_stops: Vec<String> = native_apply
            .as_ref()
            .and_then(|apply| apply["additional_stops"].as_array())
            .map(|stops| stops.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
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
            n_vocab,
            if grammar_lazy { tool_grammar } else { std::ptr::null_mut() },
        )?;
        let mut safe_sampler = SafeSampler {
            sampler: sampler_chain_raw,
        };

        let mut history: Vec<i32> = full_prompt_tokens;
        let mut utf8_buffer = Vec::new();

        let mut n_gen = 0;

        // Room for thinking = context left after the prompt minus the answer's reserve, so whatever budget
        // was chosen, the answer still fits; Low/Medium/High are shares of it (settings, Auto = High)
        let room = n_ctx.saturating_sub(n_cur.max(0) as usize + gen_reserve);
        let presets = cluaiz_core::Settings::current()
            .optimization
            .thinking_presets;
        let limit = thinking_budget.limit(room, &presets);
        let max_think_tokens = limit.tokens;
        if limit.capped {
            info!(
                "🧠 [NativeStream] Thinking budget {:?} lowered to {} tokens so the answer fits the context",
                thinking_budget, limit.tokens
            );
        }
        let mut ended_by = "model";
        let mut thought = false;
        // What is written to close thinking early: the optional budget message, then the end tag
        let close_text = match &req_budget_message {
            Some(message) => format!("{}{}", message, think_end_tag),
            None => think_end_tag.clone(),
        };
        let (root, reasoning) = crate::ffi::reasoning::ReasoningHandle::wrap(
            safe_sampler.sampler, vocab, (&think_start_tag, &think_end_tag, &close_text),
            max_think_tokens, prompt_opens_thinking,
        )?;
        safe_sampler.sampler = root;
        // The grammar sees only what the model writes outside thinking
        let mut grammar_active = false;
        let mut set_grammar = |active: bool, replay: &str| {
            if !tool_grammar.is_null() && active != grammar_active {
                crate::ffi::chat::set_tool_grammar_active(tool_grammar, vocab, active, replay);
                grammar_active = active;
            }
        };
        // The prompt's own thinking end tag (thinking off) is not output: nothing to replay
        set_grammar(!prompt_opens_thinking, "");
        // Everything generated (thinking included) for the parser, the answer part, and what of
        // the parsed answer was sent
        let mut generated = String::new();
        let mut answer_raw = String::new();
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

        reasoning.poll(skip_requested(), &mut ended_by);
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
            if llama.interrupt_signal.load(Ordering::SeqCst) {
                report.stop = "cancel";
                break;
            }

            // Upstream standard: check if token is end of generation
            if llama_cpp::llama_vocab_is_eog(vocab, next_token_id) {
                tracing::info!(
                    "🛑 [NativeStream] EOG reached for token={}. Gracefully terminating generation.",
                    next_token_id
                );
                break;
            }

            // sample_token already accepted this token into the chain (penalties see it once)
            history.push(next_token_id);

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

                    // Thinking just ended: the grammar wakes up and sees the end tag (llama-server)
                    // (a strict grammar starts at the answer itself: nothing to replay)
                    set_grammar(!in_think_block, if grammar_lazy { &think_end_tag } else { "" });
                    generated.push_str(emit_str);
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
                        if let Some(delta) = parsed.as_ref().and_then(|p| p["content"].as_str()).and_then(|c| content_stream.delta(c)) {
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
                        if !callback(emit_str.to_string()) {
                            report.stop = "cancel";
                            break;
                        }
                    }
                }
            }

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

            // Prepare single-token batch at pos n_cur
            safe_batch.batch.n_tokens = 1;
            *safe_batch.batch.token.add(0) = next_token_id;
            *safe_batch.batch.pos.add(0) = n_cur;
            *safe_batch.batch.n_seq_id.add(0) = 1;
            *(*safe_batch.batch.seq_id.add(0)).add(0) = 0;
            *safe_batch.batch.logits.add(0) = 1;

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

            n_cur += 1;

            // Sample next token
            reasoning.poll(skip_requested(), &mut ended_by);
            next_token_id =
                crate::native::sampler::sample_token(safe_sampler.sampler, checked_grammar, llama.ctx_ptr, n_vocab);
        }

        // Native tool calls: the rest of the answer, then each call as a <tool_call> block
        if let Some(parser) = &native_parser {
            if report.stop != "cancel" {
                match parser.parse(&generated, false) {
                    Ok(parsed) => {
                        let content = parsed["content"].as_str().unwrap_or_default();
                        if let Some(delta) = content_stream.delta(content) {
                            let _ = callback(delta);
                        }
                        let calls = parsed["tool_calls"].as_array().cloned().unwrap_or_default();
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
