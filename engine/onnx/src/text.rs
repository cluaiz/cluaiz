//! Real ONNX embedding execution, tokenizer inputs and explicit pooling contracts.
use crate::{media::validate_tensor, OnnxEngine};
use anyhow::{bail, Context, Result};
use cluaiz_core::contracts::normalize_embedding as normalize;
use cluaiz_core::{media::DecodedImage, media_limits::MediaLimits, EngineError};
use ort::{
    session::SessionInputValue,
    value::{Tensor, TensorElementType},
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum EmbeddingInput {
    Text(String),
    Image { image: DecodedImage, text: String },
}
pub use cluaiz_core::contracts::EmbeddingOutput;

impl OnnxEngine {
    /// Load the selected primary graph without audio-family/name routing.
    pub fn load_embedding_package(
        &mut self,
        dir: &std::path::Path,
        primary: &std::path::Path,
    ) -> Result<()> {
        let session = self.build_session(primary)?;
        for input in session.inputs() {
            if !matches!(
                input.name(),
                "input_ids" | "attention_mask" | "token_type_ids" | "pixel_values"
            ) {
                bail!("Unsupported embedding graph input: {}", input.name());
            }
        }
        self.tokenizer = if session.inputs().iter().any(|i| i.name() == "input_ids") {
            Some(Arc::new(
                tokenizers::Tokenizer::from_file(dir.join("tokenizer.json"))
                    .map_err(|e| anyhow::anyhow!("Embedding tokenizer: {e}"))?,
            ))
        } else {
            None
        };
        self.model_dir = Some(dir.to_path_buf());
        self.session_pool = vec![Arc::new(Mutex::new(session))];
        Ok(())
    }
    /// Synchronous computation; runtime calls this on its existing blocking worker.
    pub fn execute_embedding(
        &self,
        input: &EmbeddingInput,
        limits: MediaLimits,
    ) -> Result<EmbeddingOutput> {
        self.active_inferences
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let result = self.execute_embedding_inner(input, limits);
        self.active_inferences
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn execute_embedding_inner(
        &self,
        input: &EmbeddingInput,
        limits: MediaLimits,
    ) -> Result<EmbeddingOutput> {
        let session = self
            .acquire_session()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut session = session
            .lock()
            .map_err(|_| anyhow::anyhow!("Embedding session mutex poisoned"))?;
        let image = match input {
            EmbeddingInput::Image { image, .. } => Some(image),
            _ => None,
        };
        let text = match input {
            EmbeddingInput::Text(text) | EmbeddingInput::Image { text, .. } => text,
        };
        limits.check_input_bytes(text.len() as u64)?;
        let has_pixels = session.inputs().iter().any(|i| i.name() == "pixel_values");
        let has_ids = session.inputs().iter().any(|i| i.name() == "input_ids");
        if image.is_some() != has_pixels {
            bail!("Selected embedding graph/input modality mismatch; no synthetic image inputs are supplied");
        }
        if image.is_none() && (!has_ids || text.trim().is_empty()) {
            bail!("Text embedding requires non-empty text and a tokenizer graph");
        }
        if !has_ids && !text.is_empty() {
            bail!("This image graph does not consume text instructions");
        }
        let encoding = if has_ids {
            Some(
                self.tokenizer
                    .as_ref()
                    .context("Embedding tokenizer missing")?
                    .encode(text.as_str(), true)
                    .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?,
            )
        } else {
            None
        };
        let tokens = encoding.as_ref().map_or(0, |e| {
            e.get_attention_mask().iter().filter(|&&v| v != 0).count()
        });
        let mut inputs: Vec<(String, SessionInputValue<'static>)> = Vec::new();
        for graph_input in session.inputs() {
            let value: SessionInputValue<'static> = match graph_input.name() {
                "pixel_values" => {
                    let processor = crate::vision::VisionProcessor::load(
                        self.model_dir.as_ref().context("Model directory missing")?,
                    )?;
                    let (shape, pixels) = processor.prepare(
                        image.context("Image missing")?,
                        graph_input.dtype(),
                        limits,
                    )?;
                    Tensor::from_array((shape, pixels))?.into()
                }
                "input_ids" | "attention_mask" | "token_type_ids" => {
                    let encoding = encoding
                        .as_ref()
                        .context("Graph requires a tokenizer input")?;
                    let values = match graph_input.name() {
                        "input_ids" => encoding.get_ids(),
                        "attention_mask" => encoding.get_attention_mask(),
                        _ => encoding.get_type_ids(),
                    };
                    if values.is_empty() {
                        bail!("Tokenizer returned no model tokens");
                    }
                    limits.check_decoded_bytes(
                        u64::try_from(values.len())?
                            .checked_mul(8)
                            .context("Token buffer size overflow")?,
                    )?;
                    validate_tensor(
                        graph_input.dtype(),
                        TensorElementType::Int64,
                        &[1, values.len() as i64],
                    )?;
                    Tensor::from_array((
                        [1usize, values.len()],
                        values.iter().map(|&v| i64::from(v)).collect::<Vec<_>>(),
                    ))?
                    .into()
                }
                other => bail!("Unsupported embedding input {other}"),
            };
            inputs.push((graph_input.name().to_owned(), value));
        }
        let target = if image.is_some() {
            "image_embeds"
        } else {
            "text_embeds"
        };
        let named: Vec<_> = session
            .outputs()
            .iter()
            .filter(|o| o.name() == target || o.name() == "sentence_embedding")
            .collect();
        let output = match named.as_slice() {
            [one] => one.name().to_owned(),
            [] if session.outputs().len() == 1 => session.outputs()[0].name().to_owned(),
            _ => bail!("Embedding output contract is missing or ambiguous"),
        };
        let outputs = session.run(inputs)?;
        let (shape, data) = outputs[output.as_str()].try_extract_tensor::<f32>()?;
        limits.check_decoded_bytes(
            u64::try_from(data.len())?
                .checked_mul(4)
                .context("Output bytes overflow")?,
        )?;
        let mask = encoding.as_ref().map(|e| e.get_attention_mask());
        let vector = pool_output(
            shape,
            data,
            mask,
            self.model_dir.as_deref(),
            image.is_some(),
        )?;
        Ok(EmbeddingOutput {
            vector: normalize(vector)?,
            tokens,
        })
    }
    /// Legacy text-driver entry delegates to the same typed executor.
    pub fn execute_text_embedding(&self, text: &str) -> Result<Vec<f32>, EngineError> {
        self.execute_embedding(
            &EmbeddingInput::Text(text.to_owned()),
            cluaiz_core::Settings::current().media.limits,
        )
        .map(|output| output.vector)
        .map_err(|e| EngineError::EmbeddingFailed(format!("{e:#}")))
    }
}

fn pool_output(
    shape: &[i64],
    data: &[f32],
    mask: Option<&[u32]>,
    dir: Option<&std::path::Path>,
    image: bool,
) -> Result<Vec<f32>> {
    if shape.iter().any(|&v| v <= 0)
        || shape.first() != Some(&1)
        || data.iter().any(|v| !v.is_finite())
    {
        bail!("Invalid embedding output shape or non-finite data");
    }
    let expected = shape
        .iter()
        .try_fold(1usize, |count, &v| {
            count.checked_mul(usize::try_from(v).ok()?)
        })
        .context("Embedding output shape overflow")?;
    if expected != data.len() {
        bail!("Embedding output length mismatch");
    }
    match shape {
        [1, _] => Ok(data.to_vec()),
        [1, sequence, width] if !image => {
            let config: serde_json::Value = serde_json::from_slice(
                &std::fs::read(
                    dir.context("Pooling directory missing")?
                        .join("1_Pooling/config.json"),
                )
                .context("Unpooled output requires explicit 1_Pooling/config.json")?,
            )?;
            let mut modes = Vec::new();
            for (name, value) in config.as_object().context("Invalid pooling config")? {
                if name.starts_with("pooling_mode_") && value.as_bool() == Some(true) {
                    modes.push(name.as_str());
                }
            }
            let width = usize::try_from(*width)?;
            match modes.as_slice() {
                ["pooling_mode_cls_token"] => Ok(data[..width].to_vec()),
                ["pooling_mode_mean_tokens"] => {
                    let mask = mask.context("Mean pooling requires actual attention mask")?;
                    if mask.len() != usize::try_from(*sequence)? {
                        bail!("Pooling mask length mismatch");
                    }
                    let count = mask.iter().filter(|&&v| v != 0).count();
                    if count == 0 {
                        bail!("Mean pooling mask has no active tokens");
                    }
                    let mut pooled = vec![0.0; width];
                    for (row, &active) in data.chunks_exact(width).zip(mask) {
                        if active != 0 {
                            for (out, value) in pooled.iter_mut().zip(row) {
                                *out += value / count as f32;
                            }
                        }
                    }
                    Ok(pooled)
                }
                _ => bail!("Unsupported or ambiguous pooling modes"),
            }
        }
        _ => bail!("Graph output is not a supported embedding vector/sequence"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_output_rejects_logits_empty_nonfinite_and_zero_vectors() -> Result<()> {
        assert!(pool_output(&[1, 2, 3], &[1.0; 6], None, None, true).is_err());
        assert!(pool_output(&[2, 3], &[1.0; 6], None, None, false).is_err());
        assert!(pool_output(&[1, 1], &[f32::NAN], None, None, false).is_err());
        assert!(normalize(vec![0.0; 3]).is_err());
        assert_eq!(normalize(vec![3.0, 4.0])?, [0.6, 0.8]);
        Ok(())
    }
}
