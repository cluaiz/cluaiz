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
