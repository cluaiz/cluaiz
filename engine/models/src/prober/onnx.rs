//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: ONNX Model Inspection & Tensor Signatures (SSOT Binding)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct OnnxProbeResult {
    pub is_tts: bool,
    pub is_asr: bool,
    pub is_embedding: bool,
    pub is_vision: bool,
    pub tts_family: Option<String>,
}

pub struct OnnxProber;

impl OnnxProber {
    /// Inspects an ONNX model file and associated folder structures
    pub fn probe_file(path: &Path) -> OnnxProbeResult {
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let mut result = OnnxProbeResult::default();

        if filename.contains("kokoro") {
            result.is_tts = true;
            result.tts_family = Some("kokoro".to_string());
        } else if filename.contains("piper") {
            result.is_tts = true;
            result.tts_family = Some("piper".to_string());
        } else if filename.contains("cosyvoice") {
            result.is_tts = true;
            result.tts_family = Some("cosyvoice".to_string());
        } else if filename.contains("whisper") || filename.contains("sensevoice") || filename.contains("moonshine") {
            result.is_asr = true;
        } else if filename.contains("embed") || filename.contains("bge") || filename.contains("minilm") {
            result.is_embedding = true;
        } else if filename.contains("clip") || filename.contains("siglip") || filename.contains("ocr") {
            result.is_vision = true;
        }

        result
    }
}
