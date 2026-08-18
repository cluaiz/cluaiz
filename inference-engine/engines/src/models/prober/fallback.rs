//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: Fallback JSON Ingestion (config.json, tokenizer_config.json)
//! ═══════════════════════════════════════════════════════════════════════

use std::fs;
use std::path::Path;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct FallbackJsonMetadata {
    pub context_window: Option<String>,
    pub chat_template: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
    pub architecture: Option<String>,
}

pub struct FallbackProber;

impl FallbackProber {
    /// Ingests companion configuration JSON files in a model directory
    pub fn probe_directory(dir: &Path) -> FallbackJsonMetadata {
        let mut meta = FallbackJsonMetadata::default();

        // 1. Check tokenizer_config.json
        let tok_cfg = dir.join("tokenizer_config.json");
        if tok_cfg.exists() {
            if let Ok(content) = fs::read_to_string(&tok_cfg) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    if let Some(tmpl) = val.get("chat_template").and_then(|v| v.as_str()) {
                        meta.chat_template = Some(tmpl.to_string());
                        if tmpl.contains("<think>") {
                            meta.think_start_tag = Some("<think>".to_string());
                            meta.think_end_tag = Some("</think>".to_string());
                        }
                    }
                }
            }
        }

        // 2. Check config.json
        let cfg = dir.join("config.json");
        if cfg.exists() {
            if let Ok(content) = fs::read_to_string(&cfg) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    if let Some(ctx) = val.get("max_position_embeddings").and_then(|v| v.as_u64()) {
                        meta.context_window = Some(format!("{}", ctx));
                    }
                    if let Some(archs) = val.get("architectures").and_then(|v| v.as_array()) {
                        if let Some(first) = archs.first().and_then(|v| v.as_str()) {
                            meta.architecture = Some(first.to_string());
                        }
                    }
                }
            }
        }

        meta
    }
}
