use crate::ffi::llama_cpp;

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
            lookahead_logs.push(format!("🌊 Sliding Window Shift: Pruned {} tokens from KV-cache. n_cur is now {}.", n_discard, n_cur));
        }
    }
}
