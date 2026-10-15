//! 🏷️ Model Slot Type Taxonomy
//! Single Source of Truth for Chat, Ingest, Embedding, TTS, STT model slots.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SlotType {
    #[default]
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

    pub fn supported_inputs(
        &self,
        caps: &crate::taxonomy::rules::ModelCapabilities,
    ) -> Vec<String> {
        match self {
            SlotType::Chat | SlotType::Embedding => {
                let mut inputs = vec!["text".to_string()];
                if caps.has_vision {
                    inputs.push("image".to_string());
                }
                if caps.has_audio {
                    inputs.push("audio".to_string());
                }
                inputs
            }
            SlotType::Ingest => vec!["text".to_string(), "image".to_string()],
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
}
