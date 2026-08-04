/// Family 3: Supertonic — 4-Stage Iterative Latent Denoising Diffusion Pipeline
///
/// Pipeline: Text → Text Encoder + Duration Predictor → Latent Frames
///           → 10-Step Diffusion Euler Loop (vector_estimator) → Neural Vocoder → PCM
///
/// Required Package:
/// - text_encoder.onnx (encodes input text to hidden representations)
/// - duration_predictor.onnx (predicts duration of each phoneme/character)
/// - vector_estimator.onnx (iterative diffusion denoising — 10 Euler steps)
/// - vocoder.onnx / hift.onnx (converts mel spectrogram to waveform)
/// - tts.json / unicode_indexer.json (character-to-token mapping)
///
/// FACT (from audit data): 5 Supertonic repos found on HuggingFace.
/// FACT: Supertonic repos have MISSING_VOICES issues — voice files exist in repos
/// but are not bundled with the ONNX variants.
///
/// FACT (from installed model `supertonic-3`):
/// The installed model has: duration_predictor.onnx, text_encoder.onnx,
/// vector_estimator.onnx, vocoder.onnx — all 4 required stages.
///
/// Implementation Status:
/// - text_encoder: ✅ Available (installed)
/// - duration_predictor: ✅ Available (installed)  
/// - vector_estimator: ✅ Available (installed) — requires 10-step Euler ODE loop
/// - vocoder: ✅ Available (installed)
/// - Execution pipeline: ❌ Not yet wired (the 4-stage orchestration logic is missing)
///
/// The main challenge is correctly wiring:
/// 1. text_encoder output → duration_predictor input
/// 2. duration_predictor output → expand latent frames
/// 3. latent frames → 10-step Euler loop through vector_estimator
/// 4. denoised mel → vocoder → PCM

use anyhow::{anyhow, Result};

/// Execute Supertonic diffusion TTS synthesis.
///
/// Currently returns an error because the 4-stage orchestration pipeline
/// has not been implemented. All 4 ONNX sub-models are installed and available,
/// but the tensor plumbing between stages needs to be built.
pub fn execute(
    _engine: &crate::engine::OnnxEngine,
    _text: &str,
) -> Result<Vec<f32>> {
    Err(anyhow!(
        "Supertonic Diffusion TTS Pipeline Status:\n\
         ├── text_encoder.onnx:       ✅ Installed (encodes text to hidden states)\n\
         ├── duration_predictor.onnx: ✅ Installed (predicts phoneme durations)\n\
         ├── vector_estimator.onnx:   ✅ Installed (10-step Euler diffusion loop)\n\
         ├── vocoder.onnx:            ✅ Installed (mel-to-waveform)\n\
         └── Pipeline orchestration:  ❌ Not yet implemented\n\
         \n\
         All 4 ONNX graphs are installed. The missing piece is the tensor \n\
         plumbing that chains: text_encoder → duration_predictor → \n\
         vector_estimator (10 Euler steps) → vocoder.\n\
         \n\
         To use TTS now, switch to a Kokoro or VITS/Piper model."
    ))
}
