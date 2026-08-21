//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Tasks & Capability Resolution Rules (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::taxonomy::heuristics::{matches_any, CHAT_KEYWORDS};
use crate::models::taxonomy::tags::{
    HF_TAGS_CHAT, HF_TAGS_EMBEDDING, HF_TAGS_INGEST, HF_TAGS_STT, HF_TAGS_TTS,
};
use crate::models::types::entities::SlotType;

pub use cluaiz_shared::utils::ModelCapabilities;

pub struct UniversalTaskRules;

impl UniversalTaskRules {
    /// Evaluates chat-specific capability rules using SSOT heuristics
    pub fn evaluate_chat_rules(
        arch_lower: &str,
        has_chat_template: bool,
        caps: &mut ModelCapabilities,
    ) {
        if caps.explicit_tasks.iter().any(|t| t == "chat-completion" || t == "conversational") {
            caps.is_instruct = true;
        } else if caps.explicit_tasks.iter().any(|t| t == "text-generation") {
            caps.is_base = true;
        } else if has_chat_template || matches_any(arch_lower, CHAT_KEYWORDS) {
            caps.is_instruct = true;
        } else {
            caps.is_base = true;
        }

        if caps.explicit_tasks.iter().any(|t| t == "multimodal-file") || arch_lower.contains("coder") {
            caps.has_file = true;
        }
    }

    /// Returns the complete list of supported tasks for a given SlotType strictly from SSOT tags
    pub fn get_tasks_for_slot(slot_type: &SlotType, _caps: &ModelCapabilities) -> Vec<String> {
        match slot_type {
            SlotType::Chat => HF_TAGS_CHAT.iter().map(|&s| s.to_string()).collect(),
            SlotType::Ingest => HF_TAGS_INGEST.iter().map(|&s| s.to_string()).collect(),
            SlotType::Embedding => HF_TAGS_EMBEDDING.iter().map(|&s| s.to_string()).collect(),
            SlotType::Tts => HF_TAGS_TTS.iter().map(|&s| s.to_string()).collect(),
            SlotType::Stt => HF_TAGS_STT.iter().map(|&s| s.to_string()).collect(),
        }
    }

    /// Returns the canonical tasks list strictly from category name
    pub fn get_tasks_for_category(category: &str) -> Vec<String> {
        let slot = SlotType::from_category(category);
        let caps = ModelCapabilities::all_true();
        Self::get_tasks_for_slot(&slot, &caps)
    }
}
