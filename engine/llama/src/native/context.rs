use crate::ffi::llama_cpp;
use crate::native::core::NativeLlama;
use std::ffi::CString;

pub(crate) struct SafeBatch {
    pub(crate) batch: crate::ffi::llama_cpp::LlamaBatch,
}

impl Drop for SafeBatch {
    fn drop(&mut self) {
        // SAFETY: this owner releases a batch allocated by llama_batch_init once.
        unsafe {
            crate::ffi::llama_cpp::llama_batch_free(self.batch);
        }
    }
}

/// Feeds `text` (optional budget message + the thinking end tag) into the context at `pos`, as the model
/// would have written it, and leaves the logits of its last token ready, so the next sampled token starts
/// the answer (llama.cpp `--reasoning-budget` does the same). Returns the tokens fed.
///
/// # Safety
/// The caller must retain the model/context/vocab and an initialized batch whose
/// writable token/position/sequence arrays cover `llama.n_batch` entries.
pub(crate) unsafe fn close_thinking_block(
    llama: &mut NativeLlama,
    vocab: *const std::ffi::c_void,
    text: &str,
    pos: i32,
    batch: &mut crate::ffi::llama_cpp::LlamaBatch,
) -> anyhow::Result<Vec<i32>> {
    // SAFETY: caller retains a valid model/context/vocab and writable native batch for this call.
    let c_text = CString::new(text)?;
    let mut tokens = vec![0i32; text.len() + 4];
    let n = llama_cpp::llama_tokenize(
        vocab,
        c_text.as_ptr(),
        text.len() as i32,
        tokens.as_mut_ptr(),
        tokens.len() as i32,
        false,
        true, // parse special: the tag is a control token for most models
    );
    if n <= 0 {
        return Err(anyhow::anyhow!("{:?} does not tokenize", text));
    }
    tokens.truncate(n as usize);
    if pos + n >= llama.n_ctx as i32 || n > llama.n_batch as i32 {
        return Err(anyhow::anyhow!(
            "no room in the context to close the thinking block"
        ));
    }
    for (i, token) in tokens.iter().enumerate() {
        *batch.token.add(i) = *token;
        *batch.pos.add(i) = pos + i as i32;
        *batch.n_seq_id.add(i) = 1;
        *(*batch.seq_id.add(i)).add(0) = 0;
        *batch.logits.add(i) = if i + 1 == tokens.len() { 1 } else { 0 };
    }
    batch.n_tokens = n;
    if llama_cpp::llama_decode(llama.ctx_ptr, *batch) != 0 {
        return Err(anyhow::anyhow!(
            "llama_decode failed while closing the thinking block"
        ));
    }
    Ok(tokens)
}

/// 🛡️ Sliding Window / Context Shifting
/// Shifts the KV cache left to make room for new tokens and speculative drafts.
pub unsafe fn shift_context(
    ctx_ptr: *mut std::ffi::c_void,
    n_cur: &mut i32,
    n_ctx: u32,
    tokens_len: usize,
    context_shifting_mode: u8,
    lookahead_logs: &mut Vec<String>,
    force: bool,
) {
    if context_shifting_mode != 0 && (force || *n_cur >= (n_ctx as i32) - 32) {
        let shift_fraction = match context_shifting_mode {
            1 => 0.05, // Minimal (5%)
            2 => 0.10, // Standard (10%)
            3 => 0.25, // Aggressive (25%)
            4 => 0.50, // Extreme (50%)
            _ => 0.10,
        };
        let n_discard = (((n_ctx as f32) * shift_fraction) as i32).max(16);
        let n_keep = (tokens_len as i32).min((n_ctx as i32) / 2).max(1);

        if n_keep + n_discard < *n_cur {
            let mem = llama_cpp::llama_get_memory(ctx_ptr);

            // Delete oldest history tokens
            let p0_rm = n_keep;
            let p1_rm = n_keep + n_discard;
            let _rm_status = llama_cpp::llama_memory_seq_rm(mem, 0, p0_rm, p1_rm);

            // Shift remaining history left
            let p0_add = n_keep + n_discard;
            let p1_add = *n_cur;
            let delta = -n_discard;
            llama_cpp::llama_memory_seq_add(mem, 0, p0_add, p1_add, delta);

            *n_cur -= n_discard;
            lookahead_logs.push(format!(
                "🌊 Sliding Window Shift: Pruned {} tokens from KV-cache. n_cur is now {}.",
                n_discard, n_cur
            ));
        }
    }
}
