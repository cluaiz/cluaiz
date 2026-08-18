//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: ONNX Model Inspection & Tensor Signatures (SSOT Binding)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;
use cluaiz_shared::utils::model_discovery::prober::probe_weight_binary;

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
        let probe = probe_weight_binary(path, "onnx");

        let mut result = OnnxProbeResult {
            is_tts: probe.explicit_tasks.iter().any(|t| t == "text_to_speech"),
            is_asr: probe.explicit_tasks.iter().any(|t| t == "speech_to_text"),
            is_embedding: probe.explicit_tasks.iter().any(|t| t == "embedding" || t == "feature-extraction"),
            is_vision: probe.has_vision_keys || probe.has_vision_tensors,
            tts_family: None,
        };

        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        if filename.contains("kokoro") {
            result.is_tts = true;
            result.tts_family = Some("kokoro".to_string());
        } else if filename.contains("piper") {
            result.is_tts = true;
            result.tts_family = Some("piper".to_string());
        } else if filename.contains("cosyvoice") {
            result.is_tts = true;
            result.tts_family = Some("cosyvoice".to_string());
        }

        result
    }
}
