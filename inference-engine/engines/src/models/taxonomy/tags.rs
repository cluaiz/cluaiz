//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Standard HuggingFace Pipeline Tags (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

/// HuggingFace pipeline tags for Chat & Multimodal Conversational models (/v1/chat/completions)
pub const HF_TAGS_CHAT: &[&str] = &[
    "text-generation",
    "chat-completion",
    "conversational",
    "image-text-to-text",
    "image-to-text",
    "video-text-to-text",
    "audio-text-to-text",
    "visual-question-answering",
    "summarization",
    "translation",
    "any-to-any",
];

/// HuggingFace pipeline tags for Ingest / Document AI / Spatial Vision models (/v1/ingest)
pub const HF_TAGS_INGEST: &[&str] = &[
    "document-ocr",
    "document-question-answering",
    "table-extraction",
    "object-detection",
    "zero-shot-object-detection",
    "mask-generation",
    "image-segmentation",
    "instance-segmentation",
    "depth-estimation",
    "keypoint-detection",
];

/// HuggingFace pipeline tags for Multimodal & Text Embedding models (/v1/embeddings)
pub const HF_TAGS_EMBEDDING: &[&str] = &[
    "sentence-similarity",
    "feature-extraction",
    "text-classification",
    "token-classification",
    "fill-mask",
    "multiple-choice",
    "question-answering",
    "zero-shot-image-classification",
    "image-feature-extraction",
    "visual-document-retrieval",
    "keypoint-matching",
];

/// HuggingFace pipeline tags for Text-to-Speech (TTS) models (/v1/audio/speech)
pub const HF_TAGS_TTS: &[&str] = &[
    "text-to-speech",
    "voice-synthesis",
];

/// HuggingFace pipeline tags for Automatic Speech Recognition (ASR / STT) models (/v1/audio/transcriptions)
pub const HF_TAGS_STT: &[&str] = &[
    "automatic-speech-recognition",
    "audio-classification",
];
