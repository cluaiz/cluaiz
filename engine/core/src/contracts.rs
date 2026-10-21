//! 📜 Engine Traits & Contracts
//! Clean shared interfaces for Llama, ONNX, and future engines.

use anyhow::Result;

pub type StreamCallback = Box<dyn FnMut(String) -> bool + Send + 'static>;

/// Invalid embedding input, distinguished from a native model/load failure.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InvalidEmbeddingInput(pub String);

/// Actual normalized backend vector and tokenizer usage, shared across runtimes.
#[derive(Debug)]
pub struct EmbeddingOutput {
    pub vector: Vec<f32>,
    pub tokens: usize,
}

/// Normalize real backend output; empty, zero and non-finite vectors are errors.
pub fn normalize_embedding(mut vector: Vec<f32>) -> Result<Vec<f32>> {
    let norm = vector
        .iter()
        .map(|&v| f64::from(v).powi(2))
        .sum::<f64>()
        .sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        anyhow::bail!("Embedding vector has zero or invalid norm");
    }
    for value in &mut vector {
        *value = (f64::from(*value) / norm) as f32;
    }
    Ok(vector)
}

#[cfg(test)]
mod embedding_tests {
    use super::normalize_embedding;

    #[test]
    fn test_normalize_real_vectors_rejects_empty_zero_and_nonfinite() -> anyhow::Result<()> {
        for vector in [vec![], vec![0.0], vec![f32::NAN], vec![f32::INFINITY]] {
            assert!(normalize_embedding(vector).is_err());
        }
        assert_eq!(normalize_embedding(vec![3.0, 4.0])?, [0.6, 0.8]);
        assert!(normalize_embedding(vec![f32::MAX, f32::MAX])?
            .iter()
            .all(|v| v.is_finite()));
        Ok(())
    }
}

pub trait UnifiedBackend: Send + Sync {
    /// Synchronous generation (legacy or diagnostic)
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String>;

    /// Prefill prompt into context
    fn prefill(&mut self, prompt: &str) -> Result<()>;

    /// Generate an embedding vector from text
    fn embed(&mut self, _input: &str) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!(
            "Embedding generation not supported by this backend."
        ))
    }
}

pub trait StreamingInference: UnifiedBackend {
    /// High-performance token streaming
    fn generate_stream(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        callback: StreamCallback,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Audio,
    Image,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Failed to generate embedding: {0}")]
    EmbeddingFailed(String),
    #[error("Modality not supported by this engine: {0}")]
    UnsupportedModality(String),
    #[error("Internal engine error: {0}")]
    Internal(String),
}

pub trait EmbeddingDriver: Send + Sync {
    /// Generate a 1D vector embedding for text.
    fn gen_embedding(&self, text: &str) -> std::result::Result<Vec<f32>, EngineError>;

    /// Generate a multimodal embedding (e.g. from Audio bytes or Image bytes)
    fn gen_multimodal_embedding(
        &self,
        bytes: &[u8],
        modality: Modality,
        instruction: Option<String>,
    ) -> std::result::Result<Vec<f32>, EngineError>;
}
