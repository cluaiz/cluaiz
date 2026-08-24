use chrono::{DateTime, Utc};
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

    pub fn supported_tasks(&self, caps: &crate::models::taxonomy::rules::ModelCapabilities) -> Vec<String> {
        let mut tasks = Vec::new();
        match self {
            SlotType::Chat => {
                tasks.push("text-generation".to_string());
                tasks.push("chat-completion".to_string());
                if caps.has_vision {
                    tasks.push("multimodal-vision".to_string());
                }
            }
            SlotType::Embedding => {
                tasks.push("sentence-similarity".to_string());
                tasks.push("feature-extraction".to_string());
                tasks.push("embedding".to_string());
                if caps.has_vision {
                    tasks.push("vision-embedding".to_string());
                }
            }
            SlotType::Ingest => {
                tasks.push("document-ocr".to_string());
                tasks.push("table-extraction".to_string());
                tasks.push("spatial-vision".to_string());
            }
            SlotType::Tts => {
                tasks.push("text-to-speech".to_string());
                tasks.push("voice-synthesis".to_string());
            }
            SlotType::Stt => {
                tasks.push("speech_to_text".to_string());
                tasks.push("automatic-speech-recognition".to_string());
            }
        }
        tasks
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::System => write!(f, "system"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub session_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub session_id: String,
    pub message: String,
    pub role: MessageRole,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub tokens_used: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
