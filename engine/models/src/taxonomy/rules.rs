//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Tasks & Capability Resolution Rules (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::taxonomy::heuristics::{matches_any, CHAT_KEYWORDS};
use crate::models::taxonomy::tags::{
    HF_TAGS_CHAT, HF_TAGS_EMBEDDING, HF_TAGS_INGEST, HF_TAGS_STT, HF_TAGS_TTS,
};
use crate::models::types::entities::SlotType;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub is_instruct: bool,
    pub is_base: bool,
    pub is_embedding: bool,
    pub is_feature_extraction: bool,
    pub is_reranker: bool,
    pub is_tts: bool,
    pub is_asr: bool,
    pub is_audio_to_audio: bool,
    pub has_vision: bool,
    pub is_vision_chat: bool,
    pub has_file: bool,
    pub chat_completion: bool,
    pub tts_family: Option<String>,
    pub stt_family: Option<String>,
    pub explicit_tasks: Vec<String>,
}

impl ModelCapabilities {
    pub fn all_true() -> Self {
        Self {
            is_instruct: true,
            is_base: true,
            is_embedding: true,
            is_feature_extraction: true,
            is_reranker: true,
            is_tts: true,
            is_asr: true,
            is_audio_to_audio: true,
            has_vision: true,
            is_vision_chat: true,
            has_file: true,
            chat_completion: true,
            tts_family: None,
            stt_family: None,
            explicit_tasks: Vec::new(),
        }
    }
}

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
