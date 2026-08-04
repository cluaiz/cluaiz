/// Family 7: Chatterbox — Multi-Stage Semantic Generator & Neural Audio Codec
///
/// Pipeline: Text → Semantic Generator → Audio Codec → PCM
///
/// Required Package:
/// - conditional_decoder.onnx (main decoder — very large, 4.6-6.8 GB)
/// - speech_encoder.onnx (encodes reference speech for voice cloning)
/// - embed_tokens.onnx (token embedding layer)
/// - language_model.onnx (text understanding)
/// - tokenizer.json
///
/// FACT (from audit data): 5 Chatterbox repos found on HuggingFace.
/// FACT: The primary model (conditional_decoder.onnx) is 4.6-6.8 GB,
/// making it one of the largest TTS models available.
/// FACT: Chatterbox repos have multiple quantization variants (FP16, Q4, Q4F16).
///
/// Implementation Status:
/// - No Chatterbox model is currently installed locally.
/// - The pipeline requires understanding the conditional_decoder's tensor contract.
/// - Reference repos: onnx-community/chatterbox-ONNX, ResembleAI/chatterbox-turbo-ONNX

use anyhow::{anyhow, Result};

/// Execute Chatterbox TTS synthesis.
///
/// Currently returns an error because no Chatterbox model is installed
/// and the pipeline has not been implemented.
pub fn execute(
    _engine: &crate::engine::OnnxEngine,
    _text: &str,
) -> Result<Vec<f32>> {
    Err(anyhow!(
        "Chatterbox TTS Pipeline Status:\n\
         ├── conditional_decoder.onnx: ❌ Not installed (4.6-6.8 GB)\n\
         ├── speech_encoder.onnx:      ❌ Not installed\n\
         ├── embed_tokens.onnx:        ❌ Not installed\n\
         ├── language_model.onnx:      ❌ Not installed\n\
         └── Pipeline orchestration:   ❌ Not yet implemented\n\
         \n\
         Available HuggingFace repos:\n\
         - onnx-community/chatterbox-ONNX (4.63 GB)\n\
         - ResembleAI/chatterbox-turbo-ONNX (6.86 GB)\n\
         - owensong/chatterbox-nano-ONNX (0.53 GB — smallest)\n\
         \n\
         To use TTS now, switch to a Kokoro or VITS/Piper model."
    ))
}
