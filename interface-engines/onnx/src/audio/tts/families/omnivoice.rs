/// Family 8: OmniVoice — ONNX Runtime GenAI Package
///
/// Pipeline: Text → LLM Decoder → Audio Heads Decoder → Audio Embedding → PCM
///
/// Required Package:
/// - model.onnx.data / genai_config.json
/// - audio_heads_decoder.onnx (generates audio features)
/// - llm_decoder.onnx (language model backbone)
/// - audio_embeddings_encoder.onnx (encodes audio embeddings)
/// - higgs_decoder.onnx (audio tokenizer)
///
/// CRITICAL CONSTRAINT: OmniVoice requires the dedicated `onnxruntime-genai` C++ binding.
/// It CANNOT be executed via standard `ort::Session` because the GenAI runtime
/// manages its own KV-cache, attention mask, and auto-regressive generation loop.
///
/// FACT (from audit data): 2 OmniVoice repos found on HuggingFace.
/// FACT: Installed model `OmniVoice-Onnx-CUDA` has:
/// - audio_heads_decoder.onnx
/// - llm_decoder.onnx
/// - audio_embeddings_encoder.onnx
///
/// Implementation Status:
/// - Model files are partially installed (CUDA variant)
/// - Requires onnxruntime-genai C++ binding integration
/// - Standard ort::Session execution will produce garbage output

use anyhow::{anyhow, Result};

/// Execute OmniVoice TTS synthesis.
///
/// Currently returns an error because OmniVoice requires the dedicated
/// onnxruntime-genai C++ binding which is not yet integrated.
pub fn execute(
    _engine: &crate::engine::OnnxEngine,
    _text: &str,
) -> Result<Vec<f32>> {
    Err(anyhow!(
        "OmniVoice GenAI TTS Pipeline Status:\n\
         ├── audio_heads_decoder.onnx:      ✅ Installed (CUDA variant)\n\
         ├── llm_decoder.onnx:              ✅ Installed (language model)\n\
         ├── audio_embeddings_encoder.onnx: ✅ Installed\n\
         ├── onnxruntime-genai binding:     ❌ Not integrated\n\
         └── Pipeline orchestration:        ❌ Not yet implemented\n\
         \n\
         OmniVoice requires the dedicated onnxruntime-genai C++ library \n\
         for auto-regressive generation with managed KV-cache. \n\
         Standard ort::Session cannot execute this model correctly.\n\
         \n\
         To use TTS now, switch to a Kokoro or VITS/Piper model."
    ))
}
