//! ═══════════════════════════════════════════════════════════════════════
//!   Core Neural Slot Taxonomy (Universal Single Source of Truth)
//! ═══════════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SlotType {
    Chat,
    Ingest,
    Embedding,
    Tts,
    Stt,
}

impl SlotType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SlotType::Chat => "chat",
            SlotType::Ingest => "ingest",
            SlotType::Embedding => "embedding",
            SlotType::Tts => "tts",
            SlotType::Stt => "stt",
        }
    }

    pub fn from_category(cat: &str) -> Self {
        let clean = cat.to_lowercase();
        if clean.contains("embed") {
            SlotType::Embedding
        } else if clean.contains("ingest") || clean.contains("ocr") || clean.contains("vision") {
            SlotType::Ingest
        } else if clean.contains("tts") || clean.contains("audio") || clean.contains("voice") {
            SlotType::Tts
        } else if clean.contains("stt") || clean.contains("asr") || clean.contains("whisper") {
            SlotType::Stt
        } else {
            SlotType::Chat
        }
    }

    pub fn supported_inputs(&self, has_vision: bool) -> Vec<String> {
        match self {
            SlotType::Chat if has_vision => vec!["text".to_string(), "image".to_string()],
            SlotType::Chat => vec!["text".to_string()],
            SlotType::Ingest => vec!["text".to_string(), "image".to_string()],
            SlotType::Embedding if has_vision => vec!["text".to_string(), "image".to_string()],
            SlotType::Embedding => vec!["text".to_string()],
            SlotType::Tts => vec!["text".to_string()],
            SlotType::Stt => vec!["audio".to_string()],
        }
    }

    pub fn supported_outputs(&self) -> Vec<String> {
        match self {
            SlotType::Chat => vec!["text".to_string()],
            SlotType::Ingest => vec!["text".to_string()],
            SlotType::Embedding => vec!["embedding".to_string()],
            SlotType::Tts => vec!["audio".to_string()],
            SlotType::Stt => vec!["text".to_string()],
        }
    }

    pub fn supported_attachments(&self, has_vision: bool) -> Vec<String> {
        match self {
            SlotType::Chat if has_vision => vec!["image".to_string(), "pdf".to_string(), "video".to_string(), "txt".to_string()],
            SlotType::Chat => vec!["pdf".to_string(), "txt".to_string(), "code".to_string(), "markdown".to_string(), "csv".to_string(), "json".to_string()],
            SlotType::Ingest => vec!["image".to_string(), "pdf".to_string(), "video".to_string(), "txt".to_string()],
            SlotType::Embedding => vec!["txt".to_string(), "pdf".to_string(), "code".to_string(), "csv".to_string(), "json".to_string()],
            SlotType::Tts => vec!["txt".to_string()],
            SlotType::Stt => vec!["audio".to_string(), "wav".to_string(), "mp3".to_string(), "m4a".to_string(), "flac".to_string()],
        }
    }
}
