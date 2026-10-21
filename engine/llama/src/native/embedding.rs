//! Pooled GGUF text embeddings from the selected model's native context.
use super::{context::SafeBatch, NativeLlama};
use crate::ffi::llama_cpp;
use anyhow::{bail, Context, Result};
use cluaiz_core::{
    contracts::{normalize_embedding, InvalidEmbeddingInput},
    media_limits::MediaLimits,
};

/// One model/context owner. No chat generation or token-logit substitute is used.
pub struct EmbeddingModel {
    llama: NativeLlama,
    dimensions: usize,
    encoder: bool,
}

impl EmbeddingModel {
    /// Takes an embedding-enabled context; pooling stays owned by model metadata.
    pub fn new(llama: NativeLlama) -> Result<Self> {
        // SAFETY: these queries borrow handles owned by llama for the entire call.
        let (pooling, dimensions, encoder, decoder) = unsafe {
            (
                llama_cpp::llama_pooling_type(llama.ctx_ptr),
                llama_cpp::llama_model_n_embd_out(llama.model_ptr),
                llama_cpp::llama_model_has_encoder(llama.model_ptr),
                llama_cpp::llama_model_has_decoder(llama.model_ptr),
            )
        };
        // llama.h: MEAN=1, CLS=2, LAST=3. NONE and RANK are not sentence vectors.
        if !(1..=3).contains(&pooling) || (encoder && decoder) {
            bail!("Selected GGUF does not provide supported sequence embeddings");
        }
        let dimensions = usize::try_from(dimensions).context("Invalid embedding width")?;
        if dimensions == 0 {
            bail!("Embedding width is zero");
        }
        Ok(Self {
            llama,
            dimensions,
            encoder,
        })
    }

    /// Executes one whole sequence. Oversize inputs fail instead of being truncated.
    pub fn embed(&mut self, text: &str, limits: MediaLimits) -> Result<(Vec<f32>, usize)> {
        limits.check_input_bytes(u64::try_from(text.len())?)?;
        if text.trim().is_empty() {
            return Err(InvalidEmbeddingInput("Embedding text is empty".into()).into());
        }
        // SAFETY: vocabulary belongs to the live owned model; tokenizer returns owned IDs.
        let tokens = unsafe {
            super::tokenizer::tokenize(
                llama_cpp::llama_model_get_vocab(self.llama.model_ptr),
                text,
                true,
                true,
            )
        }?;
        let capacity = self.llama.n_ctx.min(self.llama.n_batch) as usize;
        if tokens.is_empty() || tokens.len() > capacity {
            return Err(InvalidEmbeddingInput(format!(
                "Embedding input has {} tokens; active sequence limit is {capacity}",
                tokens.len()
            ))
            .into());
        }
        limits.check_decoded_bytes(
            u64::try_from(tokens.len())?
                .checked_mul(4)
                .context("Token size overflow")?,
        )?;
        limits.check_decoded_bytes(
            u64::try_from(self.dimensions)?
                .checked_mul(4)
                .context("Vector size overflow")?,
        )?;
        // SAFETY: native batch factory allocates one sequence; SafeBatch frees it once.
        let mut batch = SafeBatch {
            batch: unsafe { llama_cpp::llama_batch_init(i32::try_from(tokens.len())?, 0, 1) },
        };
        // SAFETY: the batch has capacity for every token and one ID per token. Model/context
        // are exclusively borrowed. Native outputs are copied before another evaluation.
        let vector = unsafe {
            for (index, token) in tokens.iter().enumerate() {
                *batch.batch.token.add(index) = *token;
                *batch.batch.pos.add(index) = i32::try_from(index)?;
                *batch.batch.n_seq_id.add(index) = 1;
                *batch.batch.seq_id.add(index).read() = 0;
                *batch.batch.logits.add(index) = 1;
            }
            batch.batch.n_tokens = i32::try_from(tokens.len())?;
            llama_cpp::llama_memory_clear(llama_cpp::llama_get_memory(self.llama.ctx_ptr), true);
            let result = if self.encoder {
                llama_cpp::llama_encode(self.llama.ctx_ptr, batch.batch)
            } else {
                llama_cpp::llama_decode(self.llama.ctx_ptr, batch.batch)
            };
            if result != 0 {
                bail!("Native embedding evaluation failed ({result})");
            }
            let output = llama_cpp::llama_get_embeddings_seq(self.llama.ctx_ptr, 0);
            if output.is_null() {
                bail!("Native sequence embedding is unavailable");
            }
            std::slice::from_raw_parts(output, self.dimensions).to_vec()
        };
        Ok((normalize_embedding(vector)?, tokens.len()))
    }
}
