/// Family 1: Piper/VITS — Single-Stage End-to-End Variational Inference
///
/// Pipeline: Text → Phonemizer → Token IDs → model.onnx → PCM Float32 Waveform
///
/// Required Package:
/// - model.onnx (or model_q4.onnx / model.onnx.json)
/// - config.json
/// - phonemes.json / vocab.json (for PhonemeMap tokenization)
///
/// FACT: VITS is end-to-end — it generates raw PCM directly without a separate vocoder.
/// This module delegates to the existing vits_handler.rs implementation.

use anyhow::Result;
use ort::session::Session;

/// Execute VITS/Piper TTS synthesis with the given phoneme IDs.
///
/// Delegates to the existing vits_handler for ONNX session execution.
pub fn execute(
    session: &mut Session,
    phoneme_ids: &[i64],
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
    speaker_id: Option<i64>,
) -> Result<Vec<f32>> {
    crate::audio::tts::vits_handler::execute_vits(
        session,
        phoneme_ids,
        noise_scale,
        length_scale,
        noise_w,
        speaker_id,
    )
}
