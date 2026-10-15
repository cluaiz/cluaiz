use crate::ffi::llama_cpp;
use crate::native::context::{close_thinking_block, SafeBatch};
use crate::native::core::NativeLlama;
use crate::native::media::{MediaInput, Projector};
use cluaiz_models::types::StructuralDNA;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use tracing::info;

pub static mut SKIP_PTR: *const std::sync::atomic::AtomicBool = std::ptr::null();

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
        None,
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
    media: Option<(&mut MediaInput, &mut Projector)>,
) -> anyhow::Result<Vec<i32>> {
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
        if !is_pivot {
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

        let formatted_prompt = if !structured_messages.is_empty() {
            let msg_refs: Vec<(&str, &str)> = structured_messages
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
                        format!("\n{}\n{}", think_end_tag, rendered)
                    } else {
                        rendered
                    }
                }
                Ok(_) => {
                    return Err(anyhow::anyhow!(
                        "llama_chat_apply_template returned empty prompt"
                    ));
                }
                Err(e) => {
                    tracing::error!("❌ [NativeTemplate] llama.cpp chat template error: {}", e);
                    return Err(e);
                }
            }
        } else {
            // Plain text (CLI / TUI): one user message through the model's own template.
            // No caller sends pre-formatted prompts, so the text is never guessed to be one by its markers.
            let single_msg = [("user", actual_prompt.as_str())];
            match crate::native::templater::apply_chat_template(
                llama.model_ptr,
                llama.chat_templates_for(dna.chat_template.as_deref()),
                dna.chat_template.as_deref(),
                &single_msg,
                true,
                enable_thinking,
            ) {
                Ok(rendered) if !rendered.trim().is_empty() => {
                    if is_pivot && !think_end_tag.is_empty() {
                        format!("\n{}\n{}", think_end_tag, rendered)
                    } else {
                        rendered
                    }
                }
                Ok(_) => {
                    return Err(anyhow::anyhow!(
                        "llama_chat_apply_template returned empty prompt for single message"
                    ));
                }
                Err(e) => {
                    tracing::error!(
                        "❌ [NativeTemplate] llama.cpp chat template error on single message: {}",
                        e
                    );
                    return Err(e);
                }
            }
        };

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

        let vocab = llama_cpp::llama_model_get_vocab(llama.model_ptr);
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

        if n_vocab <= 0 {
            return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
        }

        tracing::debug!(
            "📝 [NativeStream] Final prompt to tokenize len={}",
            formatted_prompt.len()
        );

        let gen_reserve = (max_tokens as i32).min((llama.n_ctx as i32 / 4).clamp(256, 2048));
        let mut safe_batch = SafeBatch {
            batch: llama_cpp::llama_batch_init(llama.n_batch as i32, 0, 1),
        };
        let (full_prompt_tokens, mut n_cur) = if let Some((input, projector)) = media {
            let prefill = input.evaluate(projector, llama, &formatted_prompt, max_tokens)?;
            (prefill.text_tokens, prefill.positions)
        } else {
            let c_prompt = CString::new(formatted_prompt.clone())?;
            let mut tokens = vec![0i32; formatted_prompt.len() + 8];

            // BOS the llama.cpp way: its chat template engine strips the BOS text from the rendered prompt and
            // the tokenizer adds the BOS token when the model asks for it (llama_vocab_get_add_bos).
            // A continuation (pivot) is appended to the existing context, so it gets none.
            let add_special = !is_pivot && llama_cpp::llama_vocab_get_add_bos(vocab);

            let mut n_tokens = llama_cpp::llama_tokenize(
                vocab,
                c_prompt.as_ptr(),
                formatted_prompt.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                add_special,
                true,
            );

            if n_tokens < 0 {
                let required_size = n_tokens.abs() as usize;
                tokens.resize(required_size, 0);
                n_tokens = llama_cpp::llama_tokenize(
                    vocab,
                    c_prompt.as_ptr(),
                    formatted_prompt.len() as i32,
                    tokens.as_mut_ptr(),
                    tokens.len() as i32,
                    add_special,
                    true,
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
            if add_special && bos >= 0 && tokens.len() >= 2 && tokens[0] == bos && tokens[1] == bos
            {
                tokens.remove(0);
            }
            let full_prompt_tokens = tokens.clone(); // 🛡️ Save FULL prompt tokens BEFORE any trimming for correct return
            tracing::debug!(
                "🔢 [NativeStream] Tokenized prompt into {} tokens: {:?}",
                full_prompt_tokens.len(),
                &full_prompt_tokens[..full_prompt_tokens.len().min(16)]
            );

            let mut is_pivot = is_pivot;
            let mut has_loaded_cache = has_loaded_cache;
            let max_prompt_tokens = (llama.n_ctx as i32 - gen_reserve).max(1) as usize;

            // 🛑 ROOT FIX: If the full conversation exceeds the KV cache capacity, we CANNOT just append!
            // We MUST clear the cache, trim the oldest messages, and re-ingest from scratch at pos=0.
            if tokens.len() > max_prompt_tokens {
                is_pivot = false;
                has_loaded_cache = false;
                let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
            }

            let mut effective_cache_len = last_prefilled_tokens.len();
            let mut match_len = 0;
            // 🛡️ DYNAMIC PREFIX MATCHING (The real fix for KV cache corruption)
            if !is_pivot && has_loaded_cache {
                let min_len = last_prefilled_tokens.len().min(tokens.len());
                while match_len < min_len && last_prefilled_tokens[match_len] == tokens[match_len] {
                    match_len += 1;
                }

                tracing::debug!(
                "🔍 [KV-Debug] last_prefilled_tokens.len() = {}, tokens.len() = {}, match_len = {}",
                last_prefilled_tokens.len(),
                tokens.len(),
                match_len
            );

                if match_len < 4 {
                    tracing::info!("🧹 [KV-Reset] Match length ({}) is below threshold (4). Resetting KV cache for clean inference.", match_len);
                    let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                    llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                    tokens = full_prompt_tokens.clone();
                    has_loaded_cache = false;
                    effective_cache_len = 0;
                } else {
                    // If the match is partial, we MUST roll back the KV cache to the divergence point
                    if match_len < last_prefilled_tokens.len() {
                        let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                        llama_cpp::llama_memory_seq_rm(mem, 0, match_len as i32, -1);
                    }

                    if tokens.len() > match_len {
                        tokens = tokens[match_len..].to_vec();
                    } else {
                        tokens.clear();
                    }

                    // Update the state so start_pos is correctly set below
                    has_loaded_cache = true;
                    effective_cache_len = match_len;
                }
            }

            let mut start_pos = if is_pivot {
                llama_cpp::llama_memory_seq_pos_max(llama_cpp::llama_get_memory(llama.ctx_ptr), 0)
                    + 1
            } else if has_loaded_cache {
                effective_cache_len as i32
            } else {
                0
            };

            // 🛡️ Auto-reset KV cache if start_pos is already near context ceiling (prevents GGML stack buffer overrun ops.cpp:3767)
            if start_pos >= (llama.n_ctx as i32 - 16) {
                tracing::debug!(
                "🌊 [KV-Reset] start_pos ({}) near n_ctx ({}). Wiping KV cache for fresh prefill.",
                start_pos,
                llama.n_ctx
            );
                tracing::info!(
                "🌊 [KV-Reset] start_pos ({}) near n_ctx ({}). Wiping KV cache for fresh prefill.",
                start_pos,
                llama.n_ctx
            );
                let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                start_pos = 0;
                tokens = full_prompt_tokens.clone();
                effective_cache_len = 0;
            }

            // 🛡️ Strict GGML Buffer Overrun Guard: tokens + start_pos must never exceed llama.n_ctx - 16
            let max_allowed_tokens = (llama.n_ctx as usize).saturating_sub(start_pos as usize + 16);
            if tokens.len() > max_allowed_tokens {
                let dropped = tokens.len() - max_allowed_tokens;
                tokens.drain(0..dropped);
            }

            tracing::debug!(
                "🔍 [KV-Debug] start_pos = {}, tokens_to_decode = {}",
                start_pos,
                tokens.len()
            );

            let mut decode_failed = false;

            // 🛡️ ROOT FIX: When tokens is empty (same prompt repeated, 100% prefix match),
            // no llama_decode() runs, leaving stale logits from the previous turn's EOS token.
            // The sampler then immediately samples EOS → zero output → "No final response synthesized."
            // Fix: Re-decode the last matched token with logits=1 to refresh logits for the sampler.
            if tokens.is_empty() && effective_cache_len > 0 {
                let last_matched_pos = effective_cache_len as i32 - 1;
                let last_matched_token = full_prompt_tokens[effective_cache_len - 1];
                tracing::debug!("🔄 [KV-Fix] Tokens empty after prefix match. Re-decoding last prompt token at pos {} to refresh logits.", last_matched_pos);

                // Remove only the last position so we can re-decode it with logits=1
                let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                llama_cpp::llama_memory_seq_rm(mem, 0, last_matched_pos, last_matched_pos + 1);

                *safe_batch.batch.token.add(0) = last_matched_token;
                *safe_batch.batch.pos.add(0) = last_matched_pos;
                *safe_batch.batch.n_seq_id.add(0) = 1;
                *(*safe_batch.batch.seq_id.add(0)).add(0) = 0;
                *safe_batch.batch.logits.add(0) = 1; // MUST compute logits for sampler
                safe_batch.batch.n_tokens = 1;

                let rc = llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch);
                if rc != 0 {
                    tracing::warn!(
                    "⚠️ [KV-Fix] Logits refresh decode failed with code {}. Falling back to full prefill.",
                    rc
                );
                    decode_failed = true;
                }
            }

            if !tokens.is_empty() {
                let step = llama.prompt_chunk_len(tokens.len());
                for (chunk_idx, chunk) in tokens.chunks(step).enumerate() {
                    for (i, token) in chunk.iter().enumerate() {
                        let cur_pos =
                            (start_pos + (chunk_idx * step + i) as i32).min(llama.n_ctx as i32 - 1);
                        *safe_batch.batch.token.add(i) = *token;
                        *safe_batch.batch.pos.add(i) = cur_pos;
                        *safe_batch.batch.n_seq_id.add(i) = 1;
                        *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                        let is_last_token = (chunk_idx * step + i) == (tokens.len() - 1);
                        *safe_batch.batch.logits.add(i) = if is_last_token { 1 } else { 0 };
                    }
                    safe_batch.batch.n_tokens = chunk.len() as i32;

                    let rc = llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch);
                    if rc != 0 {
                        tracing::warn!("⚠️ [Llama-Lib] Delta prefill chunk decode failed with code {} at start_pos {}", rc, start_pos);
                        decode_failed = true;
                        break;
                    }
                }
            }

            if decode_failed {
                tracing::warn!("⚠️ [Llama-Lib] Delta prefill failed (KV cache mismatch). Falling back to full prefill from scratch...");

                // 1. Clear KV cache completely
                let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);

                // 2. Reset tokens to the full prompt and start position to 0
                tokens = full_prompt_tokens.clone();
                start_pos = 0;

                // 3. Re-decode the entire prompt
                let step = llama.prompt_chunk_len(tokens.len());
                for (chunk_idx, chunk) in tokens.chunks(step).enumerate() {
                    for (i, token) in chunk.iter().enumerate() {
                        *safe_batch.batch.token.add(i) = *token;
                        *safe_batch.batch.pos.add(i) = start_pos + (chunk_idx * step + i) as i32;
                        *safe_batch.batch.n_seq_id.add(i) = 1;
                        *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                        let is_last_token = (chunk_idx * step + i) == (tokens.len() - 1);
                        *safe_batch.batch.logits.add(i) = if is_last_token { 1 } else { 0 };
                    }
                    safe_batch.batch.n_tokens = chunk.len() as i32;

                    if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                        return Err(anyhow::anyhow!("Fatal: Full prefill fallback failed"));
                    }
                }
            }

            (full_prompt_tokens, start_pos + tokens.len() as i32)
        };
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);
        let sampler_chain_raw = crate::native::sampler::build_sampler_chain(
            dna,
            &full_prompt_tokens,
            req_samplers.as_ref(),
            n_vocab,
        )?;
        let safe_sampler = SafeSampler {
            sampler: sampler_chain_raw,
        };

        let mut history: Vec<i32> = full_prompt_tokens;
        let mut utf8_buffer = Vec::new();

        let mut n_gen = 0;

        // Room for thinking = context left after the prompt minus the answer's reserve, so whatever budget
        // was chosen, the answer still fits; Low/Medium/High are shares of it (settings, Auto = High)
        let room = (llama.n_ctx as i32 - n_cur - gen_reserve).max(0) as usize;
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

        // Sample the first token from the prompt prefill logits
        if prompt_opens_thinking {
            in_think_block = true;
            if !suppress_thinking {
                let _ = callback(think_start_tag.clone());
            }
        }

        let mut next_token_id =
            llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
        tracing::debug!(
            "🎲 [NativeStream] Initial sampled token_id: {}",
            next_token_id
        );

        while n_gen < max_tokens as i32 {
            if llama.interrupt_signal.load(Ordering::SeqCst) {
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
                            break;
                        }
                    }
                }
            }

            // ⚡ Check global UI interrupt signal to skip thinking via pointer
            let mut should_skip = false;
            if !SKIP_PTR.is_null() {
                should_skip = (*SKIP_PTR).swap(false, Ordering::SeqCst);
            }

            if in_think_block {
                think_tokens_count += 1;
                thought = true;
            } else {
                n_gen += 1;
            }
            // Budget used up, or the user pressed "skip thinking": close the block for the model
            // (after this token is decoded below), so it moves on to the answer
            let close_thinking = in_think_block
                && !think_end_tag.is_empty()
                && (should_skip || think_tokens_count >= max_think_tokens);

            // Guard against context ceiling overflow
            if n_cur >= (llama.n_ctx as i32 - 4) {
                tracing::warn!("🛑 [NativeStream] Context window ceiling reached (n_cur={}, n_ctx={}). Stopping.", n_cur, llama.n_ctx);
                break;
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
                break;
            }

            n_cur += 1;

            if close_thinking {
                ended_by = if think_tokens_count >= max_think_tokens {
                    "budget"
                } else {
                    "skip"
                };
                match close_thinking_block(llama, vocab, &close_text, n_cur, &mut safe_batch.batch)
                {
                    Ok(end_tokens) => {
                        n_cur += end_tokens.len() as i32;
                        history.extend_from_slice(&end_tokens);
                        in_think_block = false;
                        tag_window.clear();
                        info!(
                            "🧠 [NativeStream] Thinking closed after {} tokens ({}); continuing with the answer",
                            think_tokens_count, ended_by
                        );
                        if !suppress_thinking && !callback(close_text.clone()) {
                            break;
                        }
                    }
                    Err(e) => {
                        if is_media {
                            return Err(e);
                        }
                        tracing::error!(
                            "❌ [NativeStream] Could not close the thinking block: {}",
                            e
                        );
                        break;
                    }
                }
            }

            // Sample next token
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
        Ok(history)
    }
}
