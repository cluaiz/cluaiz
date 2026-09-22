use crate::backend::traits::{UnifiedBackend, StreamingInference};
use crate::backend::context::EngineContext;
use anyhow::Result;
use tokenizers::Tokenizer;

/// LinkerPlaceholder: Used to verify the Dynamic Linker Handshake.
pub struct LinkerPlaceholder;

impl UnifiedBackend for LinkerPlaceholder {
    fn generate(&mut self, _prompt: &str, _max_tokens: usize) -> std::result::Result<String, String> {
        Err("Linker handshake success. Real inference pending initialization.".to_string())
    }
    fn prefill(&mut self, _prompt: &str) -> Result<()> { Ok(()) }
    fn evaluate_tps(&self) -> f64 { 0.0 }
}

impl StreamingInference for LinkerPlaceholder {
    fn forward_raw(&mut self, _input_ids: &[u32], _pos: usize) -> Result<Vec<f32>> {
        Err(anyhow::anyhow!("Handshake Placeholder"))
    }
    fn generate_stream(
        &mut self,
        _prompt: &str,
        _max_tokens: usize,
        _callback: Box<dyn FnMut(String) -> bool + Send + 'static>,
    ) -> Result<()> {
        Err(anyhow::anyhow!("Linker handshake complete."))
    }
}
