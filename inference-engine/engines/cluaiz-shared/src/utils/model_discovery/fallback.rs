use std::path::Path;
use crate::utils::ModelCapabilities;

pub fn enrich_from_fallback_jsons(dir: &Path, caps: &mut ModelCapabilities) {
    let config_path = dir.join("config.json");
    if config_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if v.get("visual").is_some() || v.get("vision_config").is_some() {
                    caps.has_vision = true;
                    caps.is_vision_chat = true;
                }
                if v.get("audio_config").is_some() {
                    caps.has_audio = true;
                }
            }
        }
    }
    let proc_path = dir.join("processor_config.json");
    if proc_path.exists() {
        caps.has_vision = true;
    }
}
