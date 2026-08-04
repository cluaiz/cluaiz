/// Family 2: Kokoro-82M — Style-Conditioned Phoneme & Style Vector Synthesizer
///
/// Pipeline: Text → Phoneme Tokenizer → [Tokens + Style Vector] → Kokoro-82M.onnx → PCM Float32
///
/// Required Package:
/// - model.onnx (or model_uint8.onnx / model_q4.onnx)
/// - tokenizer.json
/// - voices/*.bin (style embedding vectors, e.g. af_heart.bin)
///
/// This module delegates to the existing kokoro_handler.rs implementation.

use anyhow::Result;
use ort::session::Session;

/// Execute Kokoro TTS synthesis for the given text chunks.
///
/// Delegates to the existing kokoro_handler for ONNX session execution.
/// The router handles: text chunking, G2P processing, phoneme tokenization,
/// style vector loading, and WAV encoding.
pub fn execute(
    session: &mut Session,
    token_ids: &[i64],
    style_vector: &[f32],
    speed: f32,
) -> Result<Vec<f32>> {
    // Delegate to existing implementation
    crate::audio::tts::kokoro_handler::execute_kokoro(session, token_ids, style_vector, speed)
}

/// Load a style embedding vector from the voices/ directory.
pub fn load_style_vector(model_dir: &std::path::Path, voice_name: &str) -> Result<Vec<f32>> {
    crate::audio::tts::kokoro_handler::load_style_vector(model_dir, voice_name)
}

/// Load the first available voice embedding from the voices/ directory.
pub fn load_first_available_voice(model_dir: &std::path::Path) -> Result<Vec<f32>> {
    crate::audio::tts::kokoro_handler::load_first_available_voice(model_dir)
}
