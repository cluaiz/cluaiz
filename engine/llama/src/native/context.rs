use crate::ffi::llama_cpp;

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

/// llama-server's context checkpoint: the part of the KV cache that cannot be rolled back (the
/// sliding-window cells) as it was after the first `n_tokens` positions. A later prompt that differs
/// after that point restarts there instead of processing everything again.
#[derive(Clone)]
pub struct Checkpoint {
    pub n_tokens: i32,
    pub data: Vec<u8>,
}

const STATE_SEQ_PARTIAL_ONLY: u32 = 1;
/// Checkpoints kept per context (the newest replace the oldest)
pub(crate) const MAX_CHECKPOINTS: usize = 4;
/// Prompt tokens processed after the checkpoint: llama-server checkpoints 4 tokens before the end
/// of a prompt, so the generation prompt (which history renders differently) stays after it.
pub(crate) const CHECKPOINT_TAIL: usize = 4;

/// Copies the sliding-window state now, labelled with the positions processed so far.
///
/// # Safety
/// `ctx_ptr` is a live context whose sequence 0 holds positions 0..n_tokens.
pub(crate) unsafe fn take_checkpoint(ctx_ptr: *mut std::ffi::c_void, n_tokens: i32) -> Option<Checkpoint> {
    let size = llama_cpp::llama_state_seq_get_size_ext(ctx_ptr, 0, STATE_SEQ_PARTIAL_ONLY);
    if size == 0 {
        return None;
    }
    let mut data = vec![0_u8; size];
    let written = llama_cpp::llama_state_seq_get_data_ext(ctx_ptr, data.as_mut_ptr(), size, 0, STATE_SEQ_PARTIAL_ONLY);
    (written == size).then_some(Checkpoint { n_tokens, data })
}

/// Puts a checkpoint's sliding-window state back; the caller then removes positions >= n_tokens.
///
/// # Safety
/// `ctx_ptr` is the live context the checkpoint was taken from (same model and KV layout).
pub(crate) unsafe fn load_checkpoint(ctx_ptr: *mut std::ffi::c_void, checkpoint: &Checkpoint) -> bool {
    llama_cpp::llama_state_seq_set_data_ext(
        ctx_ptr,
        checkpoint.data.as_ptr(),
        checkpoint.data.len(),
        0,
        STATE_SEQ_PARTIAL_ONLY,
    ) != 0
}

/// Adds a checkpoint, replacing one at the same position and dropping the oldest past the limit.
pub(crate) fn remember_checkpoint(list: &mut Vec<Checkpoint>, checkpoint: Checkpoint) {
    list.retain(|c| c.n_tokens != checkpoint.n_tokens);
    list.push(checkpoint);
    list.sort_by_key(|c| c.n_tokens);
    while list.len() > MAX_CHECKPOINTS {
        list.remove(0);
    }
}

/// llama-server's context shift: removes the `n_discard` KV positions after the first `n_keep` and
/// moves the following ones back, up to `n_past`. The caller removes the same range from its token list.
///
/// # Safety
/// `ctx_ptr` is a live context whose sequence 0 holds positions 0..n_past, and its memory can shift
/// (`llama_memory_can_shift`): llama.cpp aborts on a shift its memory cannot do.
pub(crate) unsafe fn shift_context(
    ctx_ptr: *mut std::ffi::c_void,
    n_past: i32,
    n_keep: i32,
    n_discard: i32,
) {
    let mem = llama_cpp::llama_get_memory(ctx_ptr);
    llama_cpp::llama_memory_seq_rm(mem, 0, n_keep, n_keep + n_discard);
    llama_cpp::llama_memory_seq_add(mem, 0, n_keep + n_discard, n_past, -n_discard);
}

/// llama-server's `--cache-reuse`: after the common prefix `n_past`, chunks of at least `min_chunk`
/// tokens that the cached prompt and the new prompt share are moved to their new positions instead of
/// being processed again (e.g. the history left after old turns were dropped). `cached` is the KV's
/// token list and is updated to the reused layout. Returns the new `n_past`.
///
/// # Safety
/// Same as [`shift_context`]; `cached` holds exactly the tokens at positions 0..cached.len().
pub(crate) unsafe fn reuse_cache_chunks(
    ctx_ptr: *mut std::ffi::c_void,
    cached: &mut Vec<i32>,
    prompt: &[i32],
    mut n_past: usize,
    min_chunk: usize,
) -> usize {
    let mem = llama_cpp::llama_get_memory(ctx_ptr);
    let (mut head_c, mut head_p) = (n_past, n_past);
    while head_c < cached.len() && head_p < prompt.len() {
        let mut n_match = 0;
        while head_c + n_match < cached.len()
            && head_p + n_match < prompt.len()
            && cached[head_c + n_match] == prompt[head_p + n_match]
        {
            n_match += 1;
        }
        if n_match >= min_chunk {
            let kv_shift = head_p as i32 - head_c as i32;
            llama_cpp::llama_memory_seq_rm(mem, 0, head_p as i32, head_c as i32);
            llama_cpp::llama_memory_seq_add(
                mem,
                0,
                head_c as i32,
                (head_c + n_match) as i32,
                kv_shift,
            );
            for i in 0..n_match {
                cached[head_p + i] = cached[head_c + i];
            }
            n_past += n_match;
            head_c += n_match;
            head_p += n_match;
        } else {
            head_c += 1;
        }
    }
    cached.truncate(n_past);
    n_past
}
