/// Family 4: Audio8 — 3-Stage Auto-Regressive Codec Transformer
///
/// Pipeline: Text → Tokenizer → Slow AR (coarse tokens) → Fast AR (fine codebook)
///           → Codec Decoder → PCM
///
/// Required Package:
/// - slow_ar_int4.onnx / slow_ar.onnx (auto-regressive coarse token generator)
/// - fast_ar_int4.onnx / fast_ar.onnx (parallel fine codebook refinement)
/// - codec_decoder_fp16.onnx (converts codec tokens to waveform)
/// - tokenizer/tokenizer.json (text tokenizer)
/// - runtime_manifest.json (execution metadata)
///
/// FACT (from installed model `Audio8-TTS-Preview-0.6B-ONNX-INT4-CUSTOM`):
/// The installed model has: slow_ar_int4.onnx, fast_ar_int4.onnx,
/// codec_decoder_fp16.onnx — all 3 required stages.
///
/// Implementation Status:
/// - slow_ar_int4: ✅ Available (installed) — auto-regressive, generates coarse tokens
/// - fast_ar_int4: ✅ Available (installed) — parallel, refines coarse tokens
/// - codec_decoder_fp16: ✅ Available (installed) — converts codec tokens to PCM
/// - Pipeline orchestration: ❌ Not yet implemented
///
/// The AR pipeline is complex:
/// 1. Tokenize text input
/// 2. Feed tokens to slow_ar in auto-regressive loop (token by token)
/// 3. Collect coarse codec codes from slow_ar output
/// 4. Feed coarse codes to fast_ar for parallel refinement
/// 5. Feed refined codes to codec_decoder for waveform generation

use anyhow::{anyhow, Result};

/// Execute Audio8 Codec-LM TTS synthesis.
///
/// Currently returns an error because the 3-stage auto-regressive pipeline
/// has not been implemented. All 3 ONNX sub-models are installed and available.
pub fn execute(
    _engine: &crate::engine::OnnxEngine,
    _text: &str,
) -> Result<Vec<f32>> {
    Err(anyhow!(
        "Audio8 Codec-LM TTS Pipeline Status:\n\
         ├── slow_ar_int4.onnx:       ✅ Installed (auto-regressive coarse token gen)\n\
         ├── fast_ar_int4.onnx:       ✅ Installed (parallel codebook refinement)\n\
         ├── codec_decoder_fp16.onnx: ✅ Installed (codec tokens → PCM waveform)\n\
         └── Pipeline orchestration:  ❌ Not yet implemented\n\
         \n\
         All 3 ONNX graphs are installed. The missing piece is the \n\
         auto-regressive token generation loop (slow_ar must run iteratively) \n\
         and the codec code handoff to fast_ar → codec_decoder.\n\
         \n\
         To use TTS now, switch to a Kokoro or VITS/Piper model."
    ))
}
