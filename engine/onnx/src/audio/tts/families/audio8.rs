//! Audio8's exported graph requires a stateful AR runtime; no synthetic codes.
use anyhow::{bail, Result};

/// The previous implementation guessed KV tensors and fabricated codec books.
/// Genuine synthesis needs FP16 caches, logits sampling, the package tokenizer,
/// prompt packing and a registered reference voice before decoding real codes.
/// Reference: Edge0-AI/Audio8_TTS, onnx_runtime/arktts_runtime/{runtime,prompt}.py.
pub fn execute(_engine: &crate::engine::OnnxEngine, _text: &str) -> Result<Vec<f32>> {
    bail!(
        "Audio8 native TTS runtime is not implemented yet for this exported FP16 KV-cache/logits contract. Real Slow AR/Fast AR generation, the package tokenizer and a registered reference voice are required. No audio was generated."
    )
}
