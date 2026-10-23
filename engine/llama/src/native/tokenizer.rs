//! One native tokenizer for chat and embedding contexts.
use crate::ffi::llama_cpp;
use std::ffi::CString;

/// The model's tokens; special-token and duplicate-BOS behavior is shared with chat.
///
/// # Safety
/// The vocabulary must belong to a live model throughout this call.
/// The loaded model's vocabulary, for counting tokens without the generation lock.
/// llama.cpp's tokenizer only reads the vocabulary; its server tokenizes on HTTP threads while a
/// slot decodes (`post_tokenize`, `handle_completions_impl`), so counting here does the same.
#[derive(Clone, Copy)]
pub struct VocabHandle(*const std::ffi::c_void);

// SAFETY: the handle is only read by llama_tokenize; its owner clears it before the model is freed.
unsafe impl Send for VocabHandle {}
unsafe impl Sync for VocabHandle {}

impl VocabHandle {
    pub(crate) fn new(vocab: *const std::ffi::c_void) -> Self {
        Self(vocab)
    }

    /// Tokens of `text` as it appears inside a rendered prompt: special tokens are parsed and no
    /// BOS is added, so the parts of a prompt add up to the whole.
    ///
    /// # Safety
    /// The model this handle came from must stay loaded throughout the call.
    pub unsafe fn count(&self, text: &str) -> anyhow::Result<usize> {
        if text.is_empty() {
            return Ok(0);
        }
        tokenize(self.0, text, false, true).map(|tokens| tokens.len())
    }
}

pub(crate) unsafe fn tokenize(
    vocab: *const std::ffi::c_void,
    text: &str,
    add_special: bool,
    parse_special: bool,
) -> anyhow::Result<Vec<i32>> {
    let text_len = i32::try_from(text.len())?;
    let c_text = CString::new(text)?;
    let capacity = i32::try_from(
        text.len()
            .checked_add(8)
            .ok_or_else(|| anyhow::anyhow!("Tokenizer capacity overflow"))?,
    )?;
    let mut tokens = vec![0i32; capacity as usize];
    // SAFETY: caller retains vocabulary; C string and output allocation live through the call.
    let mut n_tokens = llama_cpp::llama_tokenize(
        vocab,
        c_text.as_ptr(),
        text_len,
        tokens.as_mut_ptr(),
        capacity,
        add_special,
        parse_special,
    );
    if n_tokens < 0 {
        let capacity = i32::try_from(n_tokens.unsigned_abs())?;
        tokens.resize(capacity as usize, 0);
        // SAFETY: same vocabulary/text, with the capacity requested by the first native call.
        n_tokens = llama_cpp::llama_tokenize(
            vocab,
            c_text.as_ptr(),
            text_len,
            tokens.as_mut_ptr(),
            capacity,
            add_special,
            parse_special,
        );
    }
    if n_tokens < 0 {
        anyhow::bail!("Tokenization failed even after resizing buffer");
    }
    tokens.truncate(n_tokens as usize);
    // SAFETY: vocabulary is still live; this query does not allocate or retain it.
    let bos = llama_cpp::llama_vocab_bos(vocab);
    if add_special && bos >= 0 && tokens.len() >= 2 && tokens[0] == bos && tokens[1] == bos {
        tokens.remove(0);
    }
    Ok(tokens)
}
