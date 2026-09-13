use crate::ffi::llama_cpp;
use crate::native::core::NativeLlama;
use cluaiz_shared::StructuralDNA;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use tracing::{error, info, warn};

pub static mut SKIP_PTR: *const std::sync::atomic::AtomicBool = std::ptr::null();

struct SafeBatch {
    batch: crate::ffi::llama_cpp::LlamaBatch,
}

impl Drop for SafeBatch {
    fn drop(&mut self) {
        unsafe {
            crate::ffi::llama_cpp::llama_batch_free(self.batch);
        }
    }
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
    mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
) -> anyhow::Result<Vec<i32>> {
    unsafe {
        // 🛑 ROOT FIX: Reset interrupt signal when entering generation to ensure pivot works!
        llama.interrupt_signal.store(false, Ordering::SeqCst);
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
        let (req_samplers, req_think_mode, req_response_length) = if let Ok(envelope) =
            serde_json::from_str::<serde_json::Value>(&actual_prompt)
        {
            if envelope.is_object()
                && (envelope.get("messages").is_some() || envelope.get("pivot_prompt").is_some())
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
                let response_length = envelope
                    .get("response_length")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string());
                (samplers, think_mode, response_length)
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
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

        let opt_control =
            cluaiz_shared::hardware::governor::HardwareGovernor::load_optimization_settings()
                .unwrap_or_default();
        let gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();

        let effective_response_length = req_response_length
            .as_deref()
            .unwrap_or(gguf_meta.user_moved_flags.response_length.as_str())
            .to_lowercase();

        // 🧬 Primary Authority: Query llama.cpp native common_chat_templates engine using model pointer
        let (native_st, native_et) = crate::native::templater::extract_thinking_tags_native(
            llama.model_ptr,
            dna.chat_template.as_deref(),
        );

        let think_start_tag = native_st.unwrap_or_else(|| {
            if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" && !dna.think_tag_schema.contains("tool") {
                dna.think_tag_schema.clone()
            } else {
                String::new()
            }
        });

        let think_end_tag = native_et.unwrap_or_else(|| {
            if !dna.think_end_schema.is_empty() && dna.think_end_schema != "none" && !dna.think_end_schema.contains("tool") {
                dna.think_end_schema.clone()
            } else {
                String::new()
            }
        });

        let mut formatted_prompt = if !structured_messages.is_empty() {
            let msg_refs: Vec<(&str, &str)> = structured_messages
                .iter()
                .map(|(r, c)| (r.as_str(), c.as_str()))
                .collect();
            match crate::native::templater::apply_chat_template(
                llama.model_ptr,
                dna.chat_template.as_deref(),
                &msg_refs,
                true,
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
        } else if actual_prompt.contains("<|im_start|>")
            || actual_prompt.contains("<|user|>")
            || actual_prompt.contains("<start_of_turn>")
            || actual_prompt.contains("<|turn>")
            || actual_prompt.contains("[INST]")
        {
            actual_prompt.clone()
        } else {
            let single_msg = [("user", actual_prompt.as_str())];
            match crate::native::templater::apply_chat_template(
                llama.model_ptr,
                dna.chat_template.as_deref(),
                &single_msg,
                true,
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

        let tm_str = req_think_mode
            .as_deref()
            .unwrap_or(gguf_meta.user_moved_flags.think_mode.as_str())
            .to_lowercase();

        let (mut suppress_thinking, max_think_tokens) = match tm_str.as_str() {
            "off" | "false" | "0" => (true, 0),
            "low" => (false, 512),
            "medium" => (false, 1024),
            "high" | "on" | "max" => (false, usize::MAX),
            "auto" => (false, usize::MAX),
            custom_num => {
                if let Ok(budget) = custom_num.parse::<usize>() {
                    if budget == 0 {
                        (true, 0)
                    } else {
                        (false, budget)
                    }
                } else {
                    (false, usize::MAX)
                }
            }
        };

        if formatted_prompt.contains("CRITICAL INSTRUCTION")
            || (formatted_prompt.contains("<system>") && formatted_prompt.contains("\"skill\""))
        {
            suppress_thinking = true;
        }

        let mut in_think_block = false;
        let mut think_tokens_count = 0usize;
        let mut suppressed_count = 0;

        let vocab = llama_cpp::llama_model_get_vocab(llama.model_ptr);
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

        if n_vocab <= 0 {
            return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
        }

        tracing::debug!(
            "📝 [NativeStream] Final prompt to tokenize len={}",
            formatted_prompt.len()
        );

        let c_prompt = CString::new(formatted_prompt.clone())?;
        let mut tokens = vec![0i32; formatted_prompt.len() + 8];

        // Dynamic BOS handling: Only add special BOS token if model explicitly requests it
        // and the formatted prompt doesn't already begin with a special token sequence.
        let add_special = if is_pivot {
            false
        } else {
            llama_cpp::llama_vocab_get_add_bos(vocab)
                && !formatted_prompt.starts_with("<|")
                && !formatted_prompt.starts_with("<start_of_turn")
                && !formatted_prompt.starts_with("<s>")
        };

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
        let full_prompt_tokens = tokens.clone(); // 🛡️ Save FULL prompt tokens BEFORE any trimming for correct return
        eprintln!(
            "🔢 [NativeStream] Tokenized prompt into {} tokens: {:?}",
            full_prompt_tokens.len(),
            &full_prompt_tokens[..full_prompt_tokens.len().min(16)]
        );

        let mut is_pivot = is_pivot;
        let mut has_loaded_cache = has_loaded_cache;
        let gen_reserve = (max_tokens as i32).min((llama.n_ctx as i32 / 4).clamp(256, 2048));
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

            cluaiz_shared::dev_info!(
                "🔍 [KV-Debug] last_prefilled_tokens.len() = {}, tokens.len() = {}, match_len = {}",
                last_prefilled_tokens.len(),
                tokens.len(),
                match_len
            );

            if match_len < 4 {
                cluaiz_shared::dev_info!("🧹 [KV-Reset] Match length ({}) is below threshold (4). Resetting KV cache for clean inference.", match_len);
                let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
                llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);
                match_len = 0;
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

        let chunk_size = llama.n_batch as i32; // Dynamic batch/chunk size
        let mut safe_batch = SafeBatch {
            batch: llama_cpp::llama_batch_init(chunk_size, 0, 1),
        };

        let mut start_pos = if is_pivot {
            llama_cpp::llama_memory_seq_pos_max(llama_cpp::llama_get_memory(llama.ctx_ptr), 0) + 1
        } else if has_loaded_cache {
            effective_cache_len as i32
        } else {
            0
        };

        // 🛡️ Auto-reset KV cache if start_pos is already near context ceiling (prevents GGML stack buffer overrun ops.cpp:3767)
        if start_pos >= (llama.n_ctx as i32 - 16) {
            cluaiz_shared::dev_info!(
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

        cluaiz_shared::dev_info!(
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
            cluaiz_shared::dev_info!("🔄 [KV-Fix] Tokens empty after prefix match. Re-decoding last prompt token at pos {} to refresh logits.", last_matched_pos);

            // Remove only the last position so we can re-decode it with logits=1
            let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
            llama_cpp::llama_memory_seq_rm(mem, 0, last_matched_pos, last_matched_pos + 1);

            *safe_batch.batch.token.add(0) = last_matched_token;
            *safe_batch.batch.pos.add(0) = last_matched_pos;
            *safe_batch.batch.n_seq_id.add(0) = 1;
            *(*safe_batch.batch.seq_id.add(0)).add(0) = 0;
            *safe_batch.batch.logits.add(0) = 1; // MUST compute logits for sampler
            safe_batch.batch.n_tokens = 1;

            if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                cluaiz_shared::dev_info!(
                    "⚠️ [KV-Fix] Logits refresh decode failed. Falling back to full prefill."
                );
                decode_failed = true;
            }
        }

        if !tokens.is_empty() {
            for (chunk_idx, chunk) in tokens.chunks(chunk_size as usize).enumerate() {
                for (i, token) in chunk.iter().enumerate() {
                    let cur_pos = (start_pos + (chunk_idx * chunk_size as usize + i) as i32)
                        .min(llama.n_ctx as i32 - 1);
                    *safe_batch.batch.token.add(i) = *token;
                    *safe_batch.batch.pos.add(i) = cur_pos;
                    *safe_batch.batch.n_seq_id.add(i) = 1;
                    *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                    let is_last_token = (chunk_idx * chunk_size as usize + i) == (tokens.len() - 1);
                    *safe_batch.batch.logits.add(i) = if is_last_token { 1 } else { 0 };
                }
                safe_batch.batch.n_tokens = chunk.len() as i32;

                // 🌊 PCIe Direct DMA MoE Highway Hook (Prefill)
                if let Some(ref controller_arc) = llama.moe_controller {
                    if let Ok(mut ctrl) = controller_arc.lock() {
                        let token_val = *chunk.first().unwrap_or(&0);
                        let pos_val = start_pos + (chunk_idx * chunk_size as usize) as i32;
                        ctrl.pipeline_all_offloaded_chunks(token_val, pos_val);
                    }
                }

                if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                    decode_failed = true;
                    break;
                }
            }
        }

        if decode_failed {
            cluaiz_shared::dev_info!("⚠️ [Llama-Lib] Delta prefill failed (KV cache mismatch). Falling back to full prefill from scratch...");

            // 1. Clear KV cache completely
            let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
            llama_cpp::llama_memory_seq_rm(mem, 0, -1, -1);

            // 2. Reset tokens to the full prompt and start position to 0
            tokens = full_prompt_tokens.clone();
            start_pos = 0;

            // 3. Re-decode the entire prompt
            for (chunk_idx, chunk) in tokens.chunks(chunk_size as usize).enumerate() {
                for (i, token) in chunk.iter().enumerate() {
                    *safe_batch.batch.token.add(i) = *token;
                    *safe_batch.batch.pos.add(i) =
                        start_pos + (chunk_idx * chunk_size as usize + i) as i32;
                    *safe_batch.batch.n_seq_id.add(i) = 1;
                    *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                    let is_last_token = (chunk_idx * chunk_size as usize + i) == (tokens.len() - 1);
                    *safe_batch.batch.logits.add(i) = if is_last_token { 1 } else { 0 };
                }
                safe_batch.batch.n_tokens = chunk.len() as i32;

                if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                    return Err(anyhow::anyhow!("Fatal: Full prefill fallback failed"));
                }
            }
        }

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

        let mut n_cur = start_pos + tokens.len() as i32;
        let mut n_gen = 0;

        // Sample the first token from the prompt prefill logits
        let mut next_token_id =
            llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
        eprintln!(
            "🎲 [NativeStream] Initial sampled token_id: {}",
            next_token_id
        );

        while n_gen < max_tokens as i32 {
            if llama.interrupt_signal.load(Ordering::SeqCst)
                || cluaiz_shared::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst)
            {
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

            // 🎯 Feed generated token back into sampler chain so repetition penalty & penalties work
            llama_cpp::llama_sampler_accept(safe_sampler.sampler, next_token_id);
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

                    // Dynamic thinking tag state tracking
                    if !think_start_tag.is_empty() && emit_str.contains(&think_start_tag) {
                        in_think_block = true;
                    }
                    if !think_end_tag.is_empty() && emit_str.contains(&think_end_tag) {
                        in_think_block = false;
                    }

                    if in_think_block && suppress_thinking {
                        think_tokens_count += 1;
                        if think_tokens_count >= max_think_tokens {
                            in_think_block = false;
                        }
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
            unsafe {
                if !SKIP_PTR.is_null() {
                    should_skip = (*SKIP_PTR).swap(false, Ordering::SeqCst);
                } else {
                    should_skip =
                        cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.swap(false, Ordering::SeqCst);
                }
            }
            if should_skip && in_think_block {
                in_think_block = false;
            }

            if !in_think_block {
                n_gen += 1;
            } else {
                suppressed_count += 1;
                if suppressed_count >= 4096 {
                    break;
                }
            }
            cluaiz_shared::hardware::telemetry::get_pulse()
                .tps_counter
                .fetch_add(1, Ordering::SeqCst);

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

            // PCIe Direct DMA MoE Highway Hook if active
            if let Some(ref controller_arc) = llama.moe_controller {
                if let Ok(mut ctrl) = controller_arc.lock() {
                    ctrl.pipeline_all_offloaded_chunks(next_token_id, n_cur);
                }
            }

            if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                tracing::error!(
                    "❌ [NativeStream] llama_decode failed at position {}",
                    n_cur
                );
                break;
            }

            n_cur += 1;

            // Sample next token
            next_token_id =
                llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
        }

        history.truncate(n_cur as usize);
        Ok(history)
    }
}
