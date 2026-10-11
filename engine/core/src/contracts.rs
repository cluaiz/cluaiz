//! 📜 Engine Traits & Contracts
//! Clean shared interfaces for Llama, ONNX, and future engines.

use anyhow::Result;

pub type StreamCallback = Box<dyn FnMut(String) -> bool + Send + 'static>;

pub trait UnifiedBackend: Send + Sync {
    /// Synchronous generation (legacy or diagnostic)
    fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String>;

    /// Prefill prompt into context
    fn prefill(&mut self, prompt: &str) -> Result<()>;

    /// Generate an embedding vector from text
    fn embed(&mut self, _input: &str) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("Embedding generation not supported by this backend."))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

