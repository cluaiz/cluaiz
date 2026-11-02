//! ═══════════════════════════════════════════════════════════════════════
//!   Prober: Fallback JSON Ingestion (config.json, tokenizer_config.json)
//! ═══════════════════════════════════════════════════════════════════════

use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct FallbackJsonMetadata {
    pub context_window: Option<String>,
    pub chat_template: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
    pub architecture: Option<String>,
    pub vision_input: Option<bool>,
    pub audio_input: Option<bool>,
}

pub struct FallbackProber;

impl FallbackProber {
    /// Read the declared model type, including multimodal nested text configs.
    pub fn architecture_from_config(config: &Value) -> Option<&str> {
        ["/model_type", "/text_config/model_type", "/architectures/0"].into_iter()
            .filter_map(|path| config.pointer(path).and_then(Value::as_str))
            .find(|value| !value.trim().is_empty())
    }
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
                    meta.vision_input = Self::modality_config(&val, "vision_config");
                    meta.audio_input = Self::modality_config(&val, "audio_config");
                    if let Some(ctx) = val.get("max_position_embeddings").and_then(|v| v.as_u64()) {
                        meta.context_window = Some(format!("{}", ctx));
                    }
                    meta.architecture = Self::architecture_from_config(&val).map(str::to_owned);
                }
            }
        }

        meta
    }

    /// A declared encoder config is evidence; missing/malformed config stays unknown.
    fn modality_config(config: &Value, key: &str) -> Option<bool> {
        match config.get(key) {
            Some(Value::Object(fields)) if !fields.is_empty() => Some(true),
            Some(Value::Null) => Some(false),
            _ => None,
        }
    }
}
