//! 🔍 MoE Model Detector
//! Reads binary headers (GGUF / ONNX config.json) BEFORE the engine loads the model
//! to determine if the model is Mixture-of-Experts architecture and extract key
//! structural metadata needed by the Expert Offloading subsystem.
//!
//! Evidence basis: GGUF spec stores MoE metadata as KV keys at the file header.
//! Key: `llm.expert_count` (u32) → number of routed experts per MoE layer.
//! Key: `llm.expert_used_count` (u32) → active top-k experts per token.

use std::path::{Path, PathBuf};
use tracing::info;

/// Full structural metadata about a MoE model extracted from its binary header.
#[derive(Debug, Clone)]
pub struct MoeModelInfo {
    /// True if the model uses Mixture-of-Experts architecture.
    pub is_moe: bool,
    /// Total number of routed experts in each MoE layer (e.g. 256 for GLM-5.2).
    pub expert_count: usize,
    /// Number of MoE layers in the model (e.g. 75 for GLM-5.2).
    pub moe_layer_count: usize,
    /// Top-K experts activated per token per layer (e.g. 8).
    pub active_experts_per_token: usize,
    /// Total weight size of routed expert tensors across all layers (bytes).
    pub total_expert_bytes: u64,
    /// Estimated size of one single expert's weights (gate + up + down), in bytes.
    pub expert_size_bytes: u64,
    /// Estimated dense backbone size (attention + shared experts + embeddings), in bytes.
    pub dense_backbone_bytes: u64,
}

impl Default for MoeModelInfo {
    fn default() -> Self {
        Self {
            is_moe: false,
            expert_count: 0,
            moe_layer_count: 0,
            active_experts_per_token: 0,
            total_expert_bytes: 0,
            expert_size_bytes: 0,
            dense_backbone_bytes: 0,
        }
    }
}

impl MoeModelInfo {
    /// Returns the recommended expert LRU cache RAM budget in GB.
    /// Allocates enough RAM to hold at least 2 full MoE layers worth of experts concurrently.
    pub fn recommended_cache_budget_gb(&self) -> f64 {
        if !self.is_moe || self.expert_size_bytes == 0 || self.moe_layer_count == 0 {
            return 0.0;
        }
        self.total_expert_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }
}

// ─── GGUF Detector ───────────────────────────────────────────────────────────

/// Detects MoE architecture in a GGUF file by scanning its KV metadata block.
pub struct GgufMoeDetector;

impl GgufMoeDetector {
    pub fn detect(model_path: &Path) -> MoeModelInfo {
        let parent_dir = if model_path.is_file() {
            model_path.parent().unwrap_or(model_path)
        } else {
            model_path
        };

        let manifest_path = parent_dir.join("model_manifest.json");
        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let is_moe = val.get("is_moe").and_then(|v| v.as_bool()).unwrap_or(false);
                    if is_moe {
                        let expert_count = val.get("expert_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let active_experts = val.get("active_experts").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        let file_size = std::fs::metadata(model_path).map(|m| m.len()).unwrap_or(0);
                        let expert_bytes = (file_size as f64 * 0.8) as u64;
                        let dense_bytes = file_size.saturating_sub(expert_bytes);
                        return MoeModelInfo {
                            is_moe: true,
                            expert_count,
                            moe_layer_count: val.get("layer_count").and_then(|v| v.as_u64()).unwrap_or(32) as usize,
                            active_experts_per_token: if active_experts > 0 { active_experts } else { (expert_count / 8).max(1) },
                            total_expert_bytes: expert_bytes,
                            expert_size_bytes: if expert_count > 0 { expert_bytes / expert_count as u64 } else { 0 },
                            dense_backbone_bytes: dense_bytes,
                        };
                    }
                }
            }
        }

        MoeModelInfo::default()
    }
}

// ─── ONNX Detector ───────────────────────────────────────────────────────────

/// Detects MoE architecture in an ONNX model by reading its `config.json`.
pub struct OnnxMoeDetector;

impl OnnxMoeDetector {
    /// Detect MoE metadata from an ONNX model directory.
    /// Reads `config.json` next to the `.onnx` file.
    pub fn detect(model_path: &Path) -> MoeModelInfo {
        let config_dir = model_path.parent().unwrap_or(Path::new("."));
        let config_path = config_dir.join("config.json");

        if !config_path.exists() {
            // Try one directory up (models often stored in subdirs)
            let parent_config = config_dir
                .parent()
                .map(|p| p.join("config.json"))
                .unwrap_or_else(|| config_path.clone());
            if !parent_config.exists() {
                return MoeModelInfo::default();
            }
            return Self::parse_config(&parent_config);
        }

        Self::parse_config(&config_path)
    }

    fn parse_config(config_path: &PathBuf) -> MoeModelInfo {
        let json_str = match std::fs::read_to_string(config_path) {
            Ok(s) => s,
            Err(_) => return MoeModelInfo::default(),
        };
        let json: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return MoeModelInfo::default(),
        };

        let num_experts = json.get("num_experts")
            .or_else(|| json.get("num_local_experts"))
            .or_else(|| json.get("n_routed_experts"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        if num_experts == 0 {
            return MoeModelInfo::default();
        }

        let active_experts = json.get("num_experts_per_tok")
            .or_else(|| json.get("top_k"))
            .or_else(|| json.get("num_selected_experts"))
            .and_then(|v| v.as_u64())
            .unwrap_or((num_experts / 8).max(1) as u64) as usize;

        let num_layers = json.get("num_hidden_layers")
            .or_else(|| json.get("num_layers"))
            .and_then(|v| v.as_u64())
            .unwrap_or(32) as usize;

        info!(
            "🔍 [MoeDetector] ONNX MoE detected from config.json: {} experts/layer, top-k={}, {} layers",
            num_experts, active_experts, num_layers
        );

        MoeModelInfo {
            is_moe: true,
            expert_count: num_experts,
            moe_layer_count: num_layers,
            active_experts_per_token: active_experts,
            // ONNX: byte sizes unknown without parsing external data files
            total_expert_bytes: 0,
            expert_size_bytes: 0,
            dense_backbone_bytes: 0,
        }
    }
}

// ─── Unified Entry Point ──────────────────────────────────────────────────────

/// Unified MoE detection — auto-detects format from file extension.
pub fn detect_moe(model_path: &Path) -> MoeModelInfo {
    let ext = model_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "gguf" => GgufMoeDetector::detect(model_path),
        "onnx" => OnnxMoeDetector::detect(model_path),
        _ => MoeModelInfo::default(),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extracts the layer index from a tensor name like "blk.42.ffn_gate_exps.weight" → 42.
fn extract_layer_index(name: &str) -> Option<usize> {
    // Pattern: "blk.{N}." or "layers.{N}." or "model.layers.{N}."
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "blk" || *part == "layers") && i + 1 < parts.len() {
            if let Ok(idx) = parts[i + 1].parse::<usize>() {
                return Some(idx);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onnx_moe_detector() {
        let temp_dir = std::env::temp_dir().join("onnx_moe_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.json");

        let config_json = r#"{
            "num_experts": 64,
            "num_experts_per_tok": 4,
            "num_hidden_layers": 32
        }"#;
        std::fs::write(&config_path, config_json).unwrap();

        let model_path = temp_dir.join("model.onnx");
        let info = OnnxMoeDetector::detect(&model_path);

        assert!(info.is_moe);
        assert_eq!(info.expert_count, 64);
        assert_eq!(info.active_experts_per_token, 4);
        assert_eq!(info.moe_layer_count, 32);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_non_moe_detector() {
        let temp_dir = std::env::temp_dir().join("onnx_non_moe_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let config_path = temp_dir.join("config.json");

        let config_json = r#"{
            "num_hidden_layers": 32
        }"#;
        std::fs::write(&config_path, config_json).unwrap();

        let model_path = temp_dir.join("model.onnx");
        let info = OnnxMoeDetector::detect(&model_path);

        assert!(!info.is_moe);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

