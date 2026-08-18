//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Tasks & Capability Resolution Rules (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::types::entities::SlotType;

pub use cluaiz_shared::utils::ModelCapabilities;

pub struct UniversalTaskRules;

impl UniversalTaskRules {
    /// Evaluates chat-specific capability rules
    pub fn evaluate_chat_rules(
        arch_lower: &str,
        has_chat_template: bool,
        caps: &mut ModelCapabilities,
    ) {
        if caps.explicit_tasks.contains(&"chat-completion".to_string())
            || caps.explicit_tasks.contains(&"multimodal-dialogue".to_string())
        {
            caps.is_instruct = true;
        } else if caps.explicit_tasks.contains(&"text-generation".to_string()) {
            caps.is_base = true;
        } else if has_chat_template
            || arch_lower.contains("instruct")
            || arch_lower.contains("chat")
            || arch_lower.contains("-it")
        {
            caps.is_instruct = true;
        } else {
            caps.is_base = true;
        }

        if caps.explicit_tasks.contains(&"multimodal-file".to_string()) || arch_lower.contains("coder") {
            caps.has_file = true;
        }
    }

    /// Returns the complete list of supported tasks for a given SlotType and ModelCapabilities
    pub fn get_tasks_for_slot(slot_type: &SlotType, caps: &ModelCapabilities) -> Vec<String> {
        match slot_type {
            SlotType::Chat => {
                let mut tasks = Vec::new();
                if caps.is_base {
                    tasks.push("text-generation".to_string());
                }
                if caps.is_instruct || tasks.is_empty() {
                    tasks.push("chat-completion".to_string());
                }
                if caps.has_vision {
                    tasks.push("multimodal-vision".to_string());
                }
                if caps.has_video {
                    tasks.push("multimodal-video".to_string());
                }
                if caps.has_file {
                    tasks.push("multimodal-file".to_string());
                }
                if caps.has_audio {
                    tasks.push("multimodal-audio".to_string());
                }
                tasks
            }
            SlotType::VisionIngest => {
                vec![
                    "image-text-to-text".to_string(),
                    "visual-question-answering".to_string(),
                    "document-ocr".to_string(),
                    "table-extraction".to_string(),
                ]
            }
            SlotType::VisionEmbedding => {
                vec![
                    "image-feature-extraction".to_string(),
                    "zero-shot-image-classification".to_string(),
                    "visual-document-retrieval".to_string(),
                ]
            }
            SlotType::TextEmbedding => {
                vec![
                    "sentence-similarity".to_string(),
                    "feature-extraction".to_string(),
                    "embedding".to_string(),
                ]
            }
            SlotType::Tts => {
                vec![
                    "text-to-speech".to_string(),
                    "voice-synthesis".to_string(),
                ]
            }
            SlotType::Stt => {
                vec![
                    "automatic-speech-recognition".to_string(),
                    "speech-to-text".to_string(),
                ]
            }
        }
    }

    /// Returns the canonical tasks list strictly from category name
    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        let slot = SlotType::from_category(category);
        let caps = ModelCapabilities::all_true();
        Self::get_tasks_for_slot(&slot, &caps)
    }
}
