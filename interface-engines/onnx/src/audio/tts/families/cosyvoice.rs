/// Family 6: CosyVoice (1/2/3) — Multi-Stage LLM & Acoustic Flow Synthesizer
///
/// Pipeline: Text + Prompt Audio → Speech LLM → Flow Matching → HiFT Vocoder → PCM
///
/// Required Package:
/// - speech_llm.onnx (auto-regressive Speech Language Model — generates codec tokens)
/// - flow.decoder.estimator.fp32.onnx (acoustic flow-matching)
/// - hift.onnx / hift_vocoder.onnx (HiFT vocoder — converts mel to waveform)
/// - campplus.onnx (speaker embedding extractor for zero-shot voice cloning)
/// - speech_tokenizer_v2.onnx (tokenizes reference audio for LLM prompt)
///
/// FACT (from audit data): 17 CosyVoice repos found on HuggingFace.
/// FACT: ALL CosyVoice repos have PIPELINE_FRAGMENTATION issues — none contains
/// all 4 pipeline stages in a single downloadable variant. The ONNX files are
/// scattered across separate subdirectories.
///
/// Current Status:
/// - flow.decoder.estimator + hift vocoder: Available in installed model
/// - speech_llm: NOT available as standalone ONNX graph in most repos
/// - campplus: Available (speaker embedding extraction works)
/// - speech_tokenizer: Available (tokenizes reference audio)
///
/// The full CosyVoice pipeline requires the Speech LLM to generate codec tokens
/// from text, which are then fed to the flow decoder. Without the LLM stage,
/// the flow decoder has no meaningful input and produces noise.

use anyhow::{anyhow, Result};

/// Execute CosyVoice TTS synthesis.
///
/// Currently returns an error because the Speech LLM stage is not wired.
/// The flow decoder and HiFT vocoder stages exist but cannot function
/// without the LLM-generated codec tokens as input.
pub fn execute(
    _engine: &crate::engine::OnnxEngine,
    _text: &str,
) -> Result<Vec<f32>> {
    Err(anyhow!(
        "CosyVoice TTS Pipeline Status:\n\
         ├── speech_llm.onnx:     ❌ Not wired (auto-regressive LLM generates codec tokens from text)\n\
         ├── flow.decoder:        ✅ Available (acoustic flow-matching, needs LLM output as input)\n\
         ├── hift vocoder:        ✅ Available (mel-to-waveform conversion)\n\
         ├── campplus.onnx:       ✅ Available (speaker embedding for zero-shot cloning)\n\
         └── speech_tokenizer:    ✅ Available (tokenizes reference audio)\n\
         \n\
         Root Cause: The flow.decoder.estimator expects output from the Speech LLM \n\
         (codec tokens / acoustic features). Without the LLM stage, the flow decoder \n\
         has no meaningful input and produces noise.\n\
         \n\
         To use TTS now, switch to a Kokoro or VITS/Piper model."
    ))
}
