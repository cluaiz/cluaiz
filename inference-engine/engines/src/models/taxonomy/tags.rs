//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Standard HuggingFace & Machine Learning Pipeline Tags (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

/// HuggingFace pipeline tags for Text Embedding models
pub const HF_TAGS_EMBEDDING: &[&str] = &[
    "sentence-similarity",
    "feature-extraction",
];

/// HuggingFace pipeline tags for Text-to-Speech (TTS) models
pub const HF_TAGS_TTS: &[&str] = &[
    "text-to-speech",
    "text-to-audio",
];

/// HuggingFace pipeline tags for Automatic Speech Recognition (ASR / STT) models
pub const HF_TAGS_ASR: &[&str] = &[
    "automatic-speech-recognition",
];

/// HuggingFace pipeline tags for general Audio models
pub const HF_TAGS_AUDIO_GENERAL: &[&str] = &[
    "audio-classification",
    "audio-to-audio",
    "voice-activity-detection",
];

/// HuggingFace pipeline tags for Vision & Multimodal models
pub const HF_TAGS_VISION: &[&str] = &[
    "image-feature-extraction",
    "zero-shot-image-classification",
    "image-classification",
    "image-to-text",
    "image-text-to-text",
    "visual-question-answering",
    "document-question-answering",
];

/// HuggingFace pipeline tags for Chat / Causal LM models
pub const HF_TAGS_CHAT: &[&str] = &[
    "text-generation",
    "conversational",
];
