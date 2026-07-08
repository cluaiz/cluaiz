use crate::ffi::llama_cpp;
use crate::native::core::NativeLlama;
use cluaiz_shared::StructuralDNA;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use tracing::{info, error, warn};

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
    mut callback: Box<dyn FnMut(String) -> bool + Send + 'static>
) -> anyhow::Result<()> {
    unsafe {
        // 🛑 ROOT FIX: Reset interrupt signal when entering generation to ensure pivot works!
        llama.interrupt_signal.store(false, Ordering::SeqCst);
        
        let is_pivot = prompt.starts_with("[PIVOT_CONTINUE]");
        let actual_prompt = if is_pivot {
            prompt.trim_start_matches("[PIVOT_CONTINUE]").trim_start().to_string()
        } else {
            prompt.to_string()
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

        let booster = cluaiz_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();

        let templater = cluaiz_shared::prompting::templater::TemplateManager::default();
        let mut formatted_prompt = if is_pivot {
            // 🛑 ROOT FIX: If we interrupted mid-generation, the model might have been thinking.
            // Appending a new turn without closing </think> corrupts the attention map of 1-bit models.
            // We forcefully close the thought block before starting the new turn.
            format!("\n</think>\n{}", templater.format_turn(dna, &actual_prompt))
        } else {
            let mut prompt_with_constraint = actual_prompt.clone();
            
            // 🧠 Deep Truth: Dynamic Structural Constraints Injection
            if booster.think_mode == cluaiz_shared::hardware::schema::booster::FeatureState::On {
                if booster.response_length == "long" {
                    prompt_with_constraint.push_str("\n\n[SYSTEM CONSTRAINT: Think deeply and explore all possibilities. Provide a comprehensive reasoning step.]");
                } else if booster.response_length == "short" {
                    prompt_with_constraint.push_str("\n\n[SYSTEM CONSTRAINT: Keep reasoning/thinking steps brief and strictly to the point.]");
                }
            } else {
                if booster.response_length == "long" {
                    prompt_with_constraint.push_str("\n\n[SYSTEM CONSTRAINT: Provide a highly detailed, comprehensive, and exhaustive response.]");
                } else if booster.response_length == "short" {
                    prompt_with_constraint.push_str("\n\n[SYSTEM CONSTRAINT: Provide a highly concise and direct response without conversational filler.]");
                }
            }

            if booster.enforce_json {
                prompt_with_constraint.push_str("\n\n[SYSTEM CONSTRAINT: Output strictly in valid JSON format only. No markdown blocks.]");
            }

            // Avoid double formatting if the prompt was already manually formatted by the router or API
            if prompt_with_constraint.contains("<|start_header_id|>") || prompt_with_constraint.contains("<|im_start|>") {
                prompt_with_constraint
            } else {
                templater.format(dna, &prompt_with_constraint)
            }
        };

        let mut suppress_thinking = booster.think_mode == cluaiz_shared::hardware::schema::booster::FeatureState::Off;
        
        if formatted_prompt.contains("CRITICAL INSTRUCTION") || (formatted_prompt.contains("<system>") && formatted_prompt.contains("\"skill\"")) {
            suppress_thinking = true;
        }
        
        let mut think_start_tag = String::new();
        let mut think_end_tag = String::new();
        if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" {
            think_start_tag = dna.think_tag_schema.clone();
            think_end_tag = dna.think_end_schema.clone();
        }

        let mut injected_think_tag = false;
        // Only push dynamic think tag if the model supports it natively based on DNA, and it's not suppressed
        if !suppress_thinking && dna.supports_thinking && !think_start_tag.is_empty() && !formatted_prompt.contains(&think_start_tag) {
            formatted_prompt.push_str(&format!("{}\n", think_start_tag));
            injected_think_tag = true;
        }
        
        let mut in_think_block = false;
        let mut suppressed_count = 0;

        let vocab = llama_cpp::llama_model_get_vocab(llama.model_ptr);
        let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

        if n_vocab <= 0 {
            return Err(anyhow::anyhow!("💀 Invalid model vocabulary"));
        }

        let c_prompt = CString::new(formatted_prompt.clone())?;
        let mut tokens = vec![0i32; formatted_prompt.len() + 8];
        let mut n_tokens = llama_cpp::llama_tokenize(
            vocab, 
            c_prompt.as_ptr(), 
            formatted_prompt.len() as i32, 
            tokens.as_mut_ptr(), 
            tokens.len() as i32, 
            !is_pivot, // Always add BOS for full prompts to enable LCP matching against last_prefilled_tokens
            true
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
                !is_pivot, 
                true
            );
        }

        if n_tokens < 0 {
            return Err(anyhow::anyhow!("Tokenization failed even after resizing buffer"));
        }
        tokens.truncate(n_tokens as usize);

        let mut is_pivot = is_pivot;
        let mut has_loaded_cache = has_loaded_cache;
        let gen_reserve = (max_tokens as i32).min(256);
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
        // 🛡️ DYNAMIC PREFIX MATCHING (The real fix for KV cache corruption)
        if !is_pivot && has_loaded_cache {
            let mut match_len = 0;
            let min_len = last_prefilled_tokens.len().min(tokens.len());
            while match_len < min_len && last_prefilled_tokens[match_len] == tokens[match_len] {
                match_len += 1;
            }
            
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

        // 🛡️ Context Overflow Guard: Trim prompt tokens to fit within the real KV cache.
        if tokens.len() > max_prompt_tokens {
            let dropped = tokens.len() - max_prompt_tokens;
            tokens.drain(0..dropped);
        }

        let chunk_size = llama.n_batch as i32; // Dynamic batch/chunk size
        let mut safe_batch = SafeBatch { batch: llama_cpp::llama_batch_init(chunk_size, 0, 1) };

        let start_pos = if is_pivot {
            llama_cpp::llama_memory_seq_pos_max(llama_cpp::llama_get_memory(llama.ctx_ptr), 0) + 1
        } else if has_loaded_cache {
            effective_cache_len as i32
        } else {
            0
        };

        for (chunk_idx, chunk) in tokens.chunks(chunk_size as usize).enumerate() {
            for (i, token) in chunk.iter().enumerate() {
                *safe_batch.batch.token.add(i) = *token;
                *safe_batch.batch.pos.add(i) = start_pos + (chunk_idx * chunk_size as usize + i) as i32;
                *safe_batch.batch.n_seq_id.add(i) = 1;
                *(*safe_batch.batch.seq_id.add(i)).add(0) = 0;
                // Only request logits for the VERY LAST token of the ENTIRE prompt
                let is_last_token = (chunk_idx * chunk_size as usize + i) == (tokens.len() - 1);
                *safe_batch.batch.logits.add(i) = if is_last_token { 1 } else { 0 };
            }
            safe_batch.batch.n_tokens = chunk.len() as i32;

            if llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch) != 0 {
                return Err(anyhow::anyhow!("Initial decode failed at chunk {}", chunk_idx));
            }
        }

        let sampler_chain_raw = crate::native::sampler::build_sampler_chain(dna, &tokens)?;
        let safe_sampler = SafeSampler { sampler: sampler_chain_raw };

        let is_lookahead = llama.speculative_decoding_mode == 1 || llama.speculative_decoding_mode == 2;
        let mut history: Vec<i32> = tokens.clone();
        let mut lookahead_logs = Vec::new();
        let mut utf8_buffer = Vec::new();

        let mut n_cur = start_pos + tokens.len() as i32;
        let original_prompt_len = start_pos as usize + tokens.len();
        let mut n_gen = 0;

        let mut next_token_id = llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, -1);
        let mut injected_tokens_queue: std::collections::VecDeque<i32> = std::collections::VecDeque::new();

        if injected_think_tag {
            if !callback(format!("{}\n", think_start_tag)) {
                return Ok(());
            }
        }

        while n_gen < max_tokens as i32 {
            if llama.interrupt_signal.load(Ordering::SeqCst) || cluaiz_shared::GLOBAL_CANCEL_SIGNAL.load(Ordering::SeqCst) {
                break;
            }

            // ⚡ Check global UI interrupt signal to skip thinking via pointer (solves FFI state isolation)
            let mut should_skip = false;
            unsafe {
                if !SKIP_PTR.is_null() {
                    should_skip = (*SKIP_PTR).swap(false, Ordering::SeqCst);
                } else {
                    // Fallback to library-local static if pointer not set (though usually they won't match)
                    should_skip = cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.swap(false, Ordering::SeqCst);
                }
            }

            if should_skip && !think_end_tag.is_empty() {
                let force_str = format!("\n{}\n\nAnswer:\n", think_end_tag);
                let c_force = CString::new(force_str.clone()).unwrap_or_default();
                let mut force_token_arr = [0i32; 64];
                let n_force = llama_cpp::llama_tokenize(
                    vocab, c_force.as_ptr(), force_str.len() as i32,
                    force_token_arr.as_mut_ptr(), force_token_arr.len() as i32,
                    false, false // MUST BE FALSE to prevent failure on unknown pseudo-special tokens
                );
                
                if n_force > 0 {
                    for i in 0..n_force {
                        injected_tokens_queue.push_back(force_token_arr[i as usize]);
                    }
                    eprintln!("🔥 [DEBUG] INJECTED {} TOKENS!", n_force);
                } else {
                    eprintln!("🔥 [DEBUG] TOKENIZE FAILED: {}", n_force);
                }
            }

            let mut is_injecting = false;
            if let Some(injected_id) = injected_tokens_queue.pop_front() {
                next_token_id = injected_id;
                is_injecting = true;
            }

            crate::native::context::shift_context(
                llama.ctx_ptr,
                &mut n_cur,
                llama.n_ctx,
                original_prompt_len,
                llama.context_shifting_mode,
                &mut lookahead_logs,
                false
            );

            history.push(next_token_id);
            
            let mut buf = [0u8; 128];
            let n_bytes = llama_cpp::llama_token_to_piece(
                vocab, next_token_id, buf.as_mut_ptr() as *mut c_char, buf.len() as i32, 0, true
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
                    // Update in_think_block
                    if !think_start_tag.is_empty() && piece.contains(&think_start_tag) {
                        in_think_block = true;
                    }

                    let mut display_piece = piece.clone();
                    if suppress_thinking {
                        if !think_start_tag.is_empty() && display_piece.contains(&think_start_tag) {
                            display_piece = display_piece.replace(&think_start_tag, "");
                        }
                        if !think_end_tag.is_empty() && display_piece.contains(&think_end_tag) {
                            display_piece = display_piece.replace(&think_end_tag, "");
                        }
                    }

                    if !think_end_tag.is_empty() && piece.contains(&think_end_tag) {
                        in_think_block = false;
                    }

                    let mut stop_generation = false;
                    for tag in &["<turn|>", "<|im_end|>", "<end_of_turn>", "<|eot_id|>"] {
                        if display_piece.contains(tag) {
                            stop_generation = true;
                            display_piece = display_piece.replace(tag, "");
                        }
                    }
                    for tag in &["<|im_start|>", "<start_of_turn>"] {
                        display_piece = display_piece.replace(tag, "");
                    }

                    if suppress_thinking {
                        if !in_think_block && !display_piece.is_empty() {
                            if !callback(display_piece) { break; }
                        }
                    } else {
                        if !display_piece.is_empty() {
                            if !callback(display_piece) { break; }
                        }
                    }
                    if stop_generation { break; }
                }
            }

            if llama_cpp::llama_vocab_is_eog(vocab, next_token_id) {
                break;
            }

            let mut drafts = crate::native::speculative::generate_drafts(
                &history,
                vocab,
                is_lookahead,
                is_injecting,
                injected_tokens_queue.is_empty(),
                llama.n_ctx,
                n_cur,
                &mut lookahead_logs
            );

            safe_batch.batch.n_tokens = 1 + drafts.len() as i32;
            *safe_batch.batch.token.add(0) = next_token_id;
            *safe_batch.batch.pos.add(0) = n_cur;
            *safe_batch.batch.n_seq_id.add(0) = 1;
            *(*safe_batch.batch.seq_id.add(0)).add(0) = 0;
            *safe_batch.batch.logits.add(0) = 1;

            for (i, &draft_token) in drafts.iter().enumerate() {
                let idx = i + 1;
                *safe_batch.batch.token.add(idx) = draft_token;
                *safe_batch.batch.pos.add(idx) = n_cur + idx as i32;
                *safe_batch.batch.n_seq_id.add(idx) = 1;
                *(*safe_batch.batch.seq_id.add(idx)).add(0) = 0;
                *safe_batch.batch.logits.add(idx) = 1; 
            }

            let mut decode_ret = llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch);
            if decode_ret != 0 {
                if llama.context_shifting_mode != 0 {
                    eprintln!("⚠️ [Llama-Lib] Decode failed (ret={}), attempting emergency context shift...", decode_ret);
                    crate::native::context::shift_context(
                        llama.ctx_ptr,
                        &mut n_cur,
                        llama.n_ctx,
                        original_prompt_len,
                        llama.context_shifting_mode,
                        &mut lookahead_logs,
                        true
                    );
                    
                    *safe_batch.batch.pos.add(0) = n_cur;
                    for (i, _) in drafts.iter().enumerate() {
                        let idx = i + 1;
                        *safe_batch.batch.pos.add(idx) = n_cur + idx as i32;
                    }
                    
                    decode_ret = llama_cpp::llama_decode(llama.ctx_ptr, safe_batch.batch);
                    if decode_ret != 0 {
                        eprintln!("❌ [Llama-Lib] Emergency shift failed to resolve decode error (ret={}). Breaking.", decode_ret);
                        break;
                    } else {
                        eprintln!("✅ [Llama-Lib] Emergency shift successful. Generation recovered.");
                    }
                } else {
                    break;
                }
            }

            // 🌟 Shannon Entropy Gate + Logit Modifications 🌟
            let logits_ptr = llama_cpp::llama_get_logits_ith(llama.ctx_ptr, 0);
            if !logits_ptr.is_null() {
                let n_vocab = llama_cpp::llama_vocab_n_tokens(vocab);

                // 🌟 Shannon Entropy: Only sample every 16th token (monitoring only, doesn't affect output)
                // Avoids 768K float iterations (3 passes × 256K vocab) per token in debug mode
                if n_gen % 16 == 0 {
                    let logits_slice = std::slice::from_raw_parts(logits_ptr, n_vocab as usize);
                    let mut max_logit = f32::NEG_INFINITY;
                    for &l in logits_slice.iter() {
                        if l > max_logit { max_logit = l; }
                    }
                    let mut sum_exp: f64 = 0.0;
                    for &l in logits_slice.iter() {
                        sum_exp += ((l - max_logit) as f64).exp();
                    }
                    if sum_exp > 0.0 {
                        let mut entropy: f64 = 0.0;
                        for &l in logits_slice.iter() {
                            let p = ((l - max_logit) as f64).exp() / sum_exp;
                            if p > 1e-10 {
                                entropy -= p * p.log2();
                            }
                        }
                        let max_entropy = (n_vocab as f64).log2();
                        let ne = if max_entropy > 0.0 { entropy / max_entropy } else { 0.0 };
                        if ne > 0.85 {
                            info!("💥 [Shannon Gate] Entropy Spike! H(X) = {:.2}", ne);
                        }
                    }
                }

                // 🎯 Logit Bias (runs every token — affects output quality)
                if let Some(biases) = &dna.guidance_bias {
                    let logits_mut = std::slice::from_raw_parts_mut(logits_ptr, n_vocab as usize);
                    for (token_id, bias) in biases.iter() {
                        if (*token_id as usize) < logits_mut.len() {
                            logits_mut[*token_id as usize] += bias;
                        }
                    }
                }

                // 🧠 EOS Bias for "short" mode (runs every token — affects output length)
                if booster.response_length == "short" && n_gen > 30 {
                    let eos_id = llama_cpp::llama_vocab_eos(vocab);
                    if eos_id >= 0 && (eos_id as usize) < n_vocab as usize {
                        let logits_mut = std::slice::from_raw_parts_mut(logits_ptr, n_vocab as usize);
                        let bias_strength = ((n_gen - 30) as f32 * 0.15).min(5.0);
                        logits_mut[eos_id as usize] += bias_strength;
                    }
                }
            }

            n_cur += 1;
            if !in_think_block {
                n_gen += 1;
            } else {
                suppressed_count += 1;
                if suppressed_count >= 4096 {
                    break;
                }
            }
            cluaiz_shared::hardware::telemetry::get_pulse().tps_counter.fetch_add(1, Ordering::SeqCst);

            let mut n_match = 0;
            let mut eos_detected = false;
            next_token_id = llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, 0);

            for (i, &draft_token) in drafts.iter().enumerate() {
                if next_token_id == draft_token {
                    n_match += 1;
                    history.push(next_token_id);
                    
                    let n_b = llama_cpp::llama_token_to_piece(
                        vocab, next_token_id, buf.as_mut_ptr() as *mut c_char, buf.len() as i32, 0, true
                    );
                    
                    if n_b > 0 {
                        utf8_buffer.extend_from_slice(&buf[..n_b as usize]);
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
                            if !think_start_tag.is_empty() && piece.contains(&think_start_tag) {
                                in_think_block = true;
                            }

                            let mut display_piece = piece.clone();
                            if suppress_thinking {
                                if !think_start_tag.is_empty() && display_piece.contains(&think_start_tag) {
                                    display_piece = display_piece.replace(&think_start_tag, "");
                                }
                                if !think_end_tag.is_empty() && display_piece.contains(&think_end_tag) {
                                    display_piece = display_piece.replace(&think_end_tag, "");
                                }
                            }

                            if !think_end_tag.is_empty() && piece.contains(&think_end_tag) {
                                in_think_block = false;
                            }

                            let mut stop_generation = false;
                            for tag in &["<turn|>", "<|im_end|>", "<end_of_turn>", "<|eot_id|>"] {
                                if display_piece.contains(tag) {
                                    stop_generation = true;
                                    display_piece = display_piece.replace(tag, "");
                                }
                            }
                            for tag in &["<|im_start|>", "<start_of_turn>"] {
                                display_piece = display_piece.replace(tag, "");
                            }

                            if suppress_thinking {
                                if !in_think_block && !display_piece.is_empty() {
                                    if !callback(display_piece) {
                                        eos_detected = true;
                                        break;
                                    }
                                }
                            } else {
                                if !display_piece.is_empty() {
                                    if !callback(display_piece) {
                                        eos_detected = true;
                                        break;
                                    }
                                }
                            }
                            if stop_generation { 
                                eos_detected = true;
                                break; 
                            }
                        }
                    }

                    n_cur += 1;
                    if !in_think_block {
                        n_gen += 1;
                    } else {
                        suppressed_count += 1;
                        if suppressed_count >= 4096 {
                            eos_detected = true;
                            break;
                        }
                    }
                    cluaiz_shared::hardware::telemetry::get_pulse().tps_counter.fetch_add(1, Ordering::SeqCst);

                    if llama_cpp::llama_vocab_is_eog(vocab, next_token_id) {
                        eos_detected = true;
                        break;
                    }

                    next_token_id = llama_cpp::llama_sampler_sample(safe_sampler.sampler, llama.ctx_ptr, (i + 1) as i32);
                } else {
                    break;
                }
            }

            let mem = llama_cpp::llama_get_memory(llama.ctx_ptr);
            llama_cpp::llama_memory_seq_rm(mem, 0, n_cur, -1);

            if eos_detected {
                break;
            }
        }
    }

    Ok(())
}
