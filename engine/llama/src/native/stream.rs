use crate::ffi::llama_cpp;
use crate::native::context::SafeBatch;
use crate::native::core::{ContextExceeded, ContextReport, NativeLlama};
use crate::native::media::{MediaInput, Projector};
use cluaiz_models::types::StructuralDNA;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use tracing::info;

pub(crate) struct StreamOptions<'a> {
    pub media: Option<(&'a mut MediaInput, &'a mut Projector)>,
    pub skip_reasoning: Option<&'a std::sync::atomic::AtomicBool>,
}

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
        StreamOptions { media: None, skip_reasoning: None },
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
    let StreamOptions { media, skip_reasoning } = options;
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
        let (req_samplers, req_think_mode, req_reasoning_effort, _req_grammar, req_budget_message) =
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
                    let grammar = envelope
                        .get("grammar")
                        .and_then(|g| g.as_str())
                        .map(|s| s.to_string());
                    let budget_message = envelope
                        .get("reasoning_budget_message")
                        .and_then(|m| m.as_str())
                        .filter(|m| !m.is_empty())
                        .map(|m| m.to_string());
                    (
                        samplers,
                        think_mode,
                        reasoning_effort,
                        grammar,
                        budget_message,
                    )
                } else {
                    (None, None, None, None, None)
                }
            } else {
                (None, None, None, None, None)
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

        let enable_thinking = match tm_str.as_str() {
            "off" | "false" | "0" | "hide" => false,
            _ => true,
        };

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
            ..Default::default()
        };

        let render = |llama: &NativeLlama, messages: &[(String, String)]| {
            render_prompt(
                llama,
                dna,
                messages,
                enable_thinking,
                is_pivot,
                &think_end_tag,
            )
        };
        let mut formatted_prompt = render(llama, &structured_messages)?;
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
                let sizes = structured_messages
                    .iter()
                    .map(|(role, content)| {
                        Ok((role.as_str(), tokenize(vocab, content, false, false)?.len()))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
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

        let suppress_thinking = match tm_str.as_str() {
            "off" | "false" | "0" | "hide" => true,
            "on" | "true" | "1" | "show" => false,
            _ => false, // "auto" lets model natural tags emit
        };

        // Thinking budget ("Effort"); the token count is known once the prompt length is
        let thinking_budget = req_reasoning_effort.unwrap_or_default();

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
                if can_shift && n_past < cached.len() && n_past < prompt_tokens.len() {
                    n_past = crate::native::context::reuse_cache_chunks(
                        llama.ctx_ptr,
                        &mut cached,
                        &prompt_tokens,
                        n_past,
                        CACHE_REUSE_MIN_CHUNK,
                    );
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
                            info!("🧠 [Context] Restarted from the checkpoint at {} tokens (sliding window)", n_past);
                        }
                    }
                }
                // Recurrent memory cannot drop a tail: then it starts over
                if n_past == 0 || !llama_cpp::llama_memory_seq_rm(mem, 0, n_past as i32, -1) {
                    llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                    n_past = 0;
                }
                llama.checkpoints.retain(|c| c.n_tokens as usize <= n_past);
                cached.truncate(n_past);
                n_past
            };
            report.cache_n = n_past;
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

            let decode_from = |llama: &NativeLlama,
                               batch: &mut crate::ffi::llama_cpp::LlamaBatch,
                               tokens: &[i32],
                               start_pos: usize|
             -> bool {
                let step = llama.prompt_chunk_len(tokens.len());
                for (chunk_idx, chunk) in tokens.chunks(step).enumerate() {
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

            // Sliding-window models: checkpoint a few tokens before the prompt end, so the next turn
            // can restart there when its history differs from this one's generation prompt onwards
            let swa = llama_cpp::llama_model_n_swa(llama.model_ptr) > 0;
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
        report.prompt_tokens = n_cur.max(0) as usize;
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);
        let sampler_chain_raw = crate::native::sampler::build_sampler_chain(
            dna,
            &full_prompt_tokens,
            req_samplers.as_ref(),
            n_vocab,
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
            llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
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

            // llama_sampler_sample already accepted this token into the chain (penalties see it once)
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

                    if in_think_block && suppress_thinking {
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
                llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
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

/// The model's tokens for `text`. `add_special` adds BOS when the model wants one; `parse_special`
/// reads control tokens written as text (rendered templates), off for raw message content.
///
/// # Safety
/// `vocab` belongs to a loaded model.
unsafe fn tokenize(
    vocab: *const std::ffi::c_void,
    text: &str,
    add_special: bool,
    parse_special: bool,
) -> anyhow::Result<Vec<i32>> {
    let c_text = CString::new(text)?;
    let mut tokens = vec![0i32; text.len() + 8];
    let mut n_tokens = llama_cpp::llama_tokenize(
        vocab,
        c_text.as_ptr(),
        text.len() as i32,
        tokens.as_mut_ptr(),
        tokens.len() as i32,
        add_special,
        parse_special,
    );
    if n_tokens < 0 {
        tokens.resize(n_tokens.unsigned_abs() as usize, 0);
        n_tokens = llama_cpp::llama_tokenize(
            vocab,
            c_text.as_ptr(),
            text.len() as i32,
            tokens.as_mut_ptr(),
            tokens.len() as i32,
            add_special,
            parse_special,
        );
    }
    if n_tokens < 0 {
        return Err(anyhow::anyhow!(
            "Tokenization failed even after resizing buffer"
        ));
    }
    tokens.truncate(n_tokens as usize);
    // A prompt that already carried the BOS text now holds it twice: keep one (compared by token id)
    let bos = llama_cpp::llama_vocab_bos(vocab);
    if add_special && bos >= 0 && tokens.len() >= 2 && tokens[0] == bos && tokens[1] == bos {
        tokens.remove(0);
    }
    Ok(tokens)
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
