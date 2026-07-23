use anyhow::Result;

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

/// The Core Architecture Trait for Pluggable Routing
pub trait EmbeddingDriver: Send + Sync {
    /// Generate a 1D vector embedding for text.
    fn gen_embedding(&self, text: &str) -> Result<Vec<f32>, EngineError>;
    
    /// Generate a multimodal embedding (e.g. from Audio bytes or Image bytes)
    fn gen_multimodal_embedding(&self, bytes: &[u8], modality: Modality, instruction: Option<String>) -> Result<Vec<f32>, EngineError>;
}
