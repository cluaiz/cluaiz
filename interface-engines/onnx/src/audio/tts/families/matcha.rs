/// Family 5: Matcha-TTS — 2-Stage Acoustic Flow-Matching Pipeline
///
/// Pipeline: Text → Flow Estimator → Mel Spectrogram [1, 80, T] → HiFi-GAN → PCM
///
/// Required Package:
/// - matcha_acoustic.onnx (or flow.decoder.estimator.onnx)
/// - hifigan_vocoder.onnx (or vocoder.onnx)
/// - config.json
///
/// This module delegates to the existing flow_matching.rs implementation.

use anyhow::Result;
use ort::session::Session;

/// Execute Matcha-TTS flow-matching synthesis.
///
/// Delegates to the existing FlowMatchingSampler for the ODE sampling loop,
/// then runs the neural vocoder for mel-to-PCM conversion.
pub fn sample_mel_features_with_text(
    session: &mut Session,
    engine: &crate::engine::OnnxEngine,
    tokenizer: Option<&tokenizers::Tokenizer>,
    text_input: &str,
    seq_len: usize,
    num_steps: usize,
) -> Result<Vec<f32>> {
    crate::audio::tts::flow_matching::FlowMatchingSampler::sample_mel_features_with_text(
        session, engine, tokenizer, text_input, seq_len, num_steps,
    )
}
