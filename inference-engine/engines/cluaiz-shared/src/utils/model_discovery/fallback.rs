use std::path::Path;
use crate::utils::ModelCapabilities;

pub struct ExtraMetadataFallback {
    pub chat_template: Option<String>,
    pub think_start_tag: Option<String>,
    pub think_end_tag: Option<String>,
}

pub fn enrich_from_fallback_jsons(dir: &Path, caps: &mut ModelCapabilities) -> ExtraMetadataFallback {
    let mut extra = ExtraMetadataFallback {
        chat_template: None,
        think_start_tag: None,
        think_end_tag: None,
    };

    let config_path = dir.join("config.json");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                // Vision config detection
                if v.get("visual").is_some() || v.get("vision_config").is_some() || v.get("image_size").is_some() {
                    caps.has_vision = true;
                    if caps.is_instruct {
                        caps.is_vision_chat = true;
                    }
                }
                // Audio config detection
                if v.get("audio_config").is_some() || v.get("speech_config").is_some() {
                    caps.has_audio = true;
                }
                // Architectures array inspection
                if let Some(archs) = v.get("architectures").and_then(|a| a.as_array()) {
                    for arch in archs {
                        if let Some(arch_str) = arch.as_str() {
                            let arch_lower = arch_str.to_lowercase();
                            if arch_lower.contains("whisper") {
                                caps.has_audio = true;
                                caps.is_asr = true;
                            } else if arch_lower.contains("kokoro") || arch_lower.contains("bark") || arch_lower.contains("vits") {
                                caps.has_audio = true;
                                caps.is_tts = true;
                            } else if arch_lower.contains("bert") || arch_lower.contains("embedding") {
                                caps.is_embedding = true;
                                caps.is_feature_extraction = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let proc_path = dir.join("processor_config.json");
    let preproc_path = dir.join("preprocessor_config.json");
    if proc_path.exists() || preproc_path.exists() {
        caps.has_vision = true;
    }

    // Parse tokenizer_config.json & chat_template.json for fallback chat_template & special reasoning tokens
    let tok_config_path = dir.join("tokenizer_config.json");
    if tok_config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&tok_config_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(tmpl) = v.get("chat_template").and_then(|t| t.as_str()) {
                    extra.chat_template = Some(tmpl.to_string());
                }

                // Dynamic key-driven special tokens extraction from tokenizer_config.json
                if let Some(spec_map) = v.get("special_tokens_map").and_then(|s| s.as_object()) {
                    for (key, val) in spec_map {
                        let key_lower = key.to_lowercase();
                        if let Some(tok_str) = val.as_str() {
                            if key_lower.contains("think_start") || key_lower.contains("reasoning_start") {
                                extra.think_start_tag = Some(tok_str.to_string());
                            } else if key_lower.contains("think_end") || key_lower.contains("reasoning_end") {
                                extra.think_end_tag = Some(tok_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Dynamic special_tokens_map.json file parsing
    let spec_map_file = dir.join("special_tokens_map.json");
    if spec_map_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&spec_map_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(spec_map) = v.as_object() {
                    for (key, val) in spec_map {
                        let key_lower = key.to_lowercase();
                        if let Some(tok_str) = val.as_str() {
                            if key_lower.contains("think_start") || key_lower.contains("reasoning_start") {
                                extra.think_start_tag = Some(tok_str.to_string());
                            } else if key_lower.contains("think_end") || key_lower.contains("reasoning_end") {
                                extra.think_end_tag = Some(tok_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let chat_tmpl_file = dir.join("chat_template.json");
    if extra.chat_template.is_none() && chat_tmpl_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&chat_tmpl_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(tmpl) = v.get("chat_template").and_then(|t| t.as_str()) {
                    extra.chat_template = Some(tmpl.to_string());
                }
            } else {
                extra.chat_template = Some(content);
            }
        }
    }

    extra
}
