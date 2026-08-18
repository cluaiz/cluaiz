//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: GGUF Header & KV Metadata Probing (SSOT Binding)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::Path;
use cluaiz_shared::utils::model_discovery::prober::probe_weight_binary;

#[derive(Debug, Clone, Default)]
pub struct GgufProbeResult {
    pub architecture: Option<String>,
    pub context_window: Option<String>,
    pub chat_template: Option<String>,
    pub quantization: Option<String>,
    pub parameter_count: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
    pub is_embedding: bool,
    pub has_vision: bool,
}

pub struct GgufProber;

impl GgufProber {
    /// Probes a GGUF file and extracts architectural metadata using cluaiz-shared SSOT engine
    pub fn probe_file(path: &Path) -> Result<GgufProbeResult, String> {
        let probe = probe_weight_binary(path, "gguf");

        Ok(GgufProbeResult {
            architecture: if probe.architecture != "Unknown" { Some(probe.architecture) } else { None },
            context_window: if probe.context_window != "Unknown" { Some(probe.context_window) } else { None },
            chat_template: probe.chat_template,
            quantization: probe.quantization,
            parameter_count: if probe.parameters_str != "Unknown" { Some(probe.parameters_str) } else { None },
            think_start_tag: probe.think_start_tag,
            think_end_tag: probe.think_end_tag,
            is_embedding: probe.has_pooling || probe.explicit_tasks.iter().any(|t| t == "embedding" || t == "feature-extraction"),
            has_vision: probe.has_vision_keys || probe.has_vision_tensors,
        })
    }
}
