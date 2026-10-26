//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Universal Model Classifier (Single Source of Truth)
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::types::entities::SlotType;
use crate::models::taxonomy::rules::ModelCapabilities;
use crate::models::taxonomy::tags::*;

#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub slot: SlotType,
    pub category: String,
    pub supported_tasks: Vec<String>,
    pub capabilities: ModelCapabilities,
    pub tts_family: Option<String>,
}

pub struct UniversalModelClassifier;

impl UniversalModelClassifier {
    /// Classifies a model from the tasks it declares: its Hugging Face pipeline tag and tags, or
    /// the tasks read from its weights (GGUF header, ONNX graph). Names of models and files are
    /// never read. A model that declares nothing is a chat model; the TTS family is read from
    /// the model's registry entry or its graph when it loads.
    pub fn classify(pipeline_tag: Option<&str>, additional_tags: &[String]) -> ClassificationResult {
        let tag_clean = pipeline_tag.unwrap_or("").trim().to_lowercase();
        let declares = |tags: &[&str]| tags.iter().any(|&t| tag_clean == t) || additional_tags.iter().any(|a| tags.iter().any(|&t| a.eq_ignore_ascii_case(t)));
        let result = |slot: SlotType, category: &str, tasks: &[&str], capabilities: ModelCapabilities| ClassificationResult {
            slot,
            category: category.to_string(),
            supported_tasks: tasks.iter().map(|t| t.to_string()).collect(),
            capabilities,
            tts_family: None,
        };

        if declares(HF_TAGS_TTS) {
            let caps = ModelCapabilities { is_tts: true, ..Default::default() };
            return result(SlotType::Tts, "tts", &["text-to-speech", "voice-synthesis"], caps);
        }
        if declares(HF_TAGS_STT) {
            let caps = ModelCapabilities { is_asr: true, ..Default::default() };
            return result(SlotType::Stt, "stt", &["automatic-speech-recognition", "audio-classification"], caps);
        }
        if declares(HF_TAGS_EMBEDDING) || declares(&["embedding", "embeddings"]) {
            let caps = ModelCapabilities { is_embedding: true, is_feature_extraction: true, ..Default::default() };
            return result(SlotType::Embedding, "embedding", &["sentence-similarity", "feature-extraction", "visual-document-retrieval"], caps);
        }
        if declares(HF_TAGS_INGEST) {
            let caps = ModelCapabilities { has_vision: true, ..Default::default() };
            return result(SlotType::Ingest, "ingest", &["document-ocr", "table-extraction", "object-detection"], caps);
        }

        let is_vlm = declares(&["image-text-to-text", "image-to-text", "visual-question-answering"]);
        let mut caps = ModelCapabilities { chat_completion: true, ..Default::default() };
        let mut tasks = vec!["chat-completion", "text-generation"];
        if is_vlm {
            caps.has_vision = true;
            caps.is_vision_chat = true;
            tasks.extend(["image-text-to-text", "image-to-text", "visual-question-answering"]);
        }
        result(SlotType::Chat, "chat", &tasks, caps)
    }
}
