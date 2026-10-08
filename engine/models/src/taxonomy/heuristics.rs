//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Heuristic Matchers & Architectural Keyword Matrices (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

// ─── 1. Chat & Multimodal Conversational Slot (/v1/chat/completions) ───
pub const CHAT_KEYWORDS: &[&str] = &[
    "instruct",
    "chat",
    "-it",
    "dialogue",
    "assistant",
    "conversational",
];

pub const MULTIMODAL_CHAT_KEYWORDS: &[&str] = &[
    "qwen-vl",
    "qwen2-vl",
    "llava",
    "internvl",
    "phi-3-vision",
    "paligemma",
    "minicpm-v",
    "llama-vision",
    "pixtral",
];

// ─── 2. Ingest Document AI / OCR / Spatial Vision Slot (/v1/ingest) ────
pub const INGEST_KEYWORDS: &[&str] = &[
    "got-ocr",
    "nougat",
    "surya",
    "florence",
    "table-transformer",
    "detr",
    "sam-2",
    "sam2",
    "depth-anything",
    "document-ocr",
];

// ─── 3. Unified Multimodal & Text Embedding Slot (/v1/embeddings) ──────
pub const EMBEDDING_KEYWORDS: &[&str] = &[
    "embed",
    "embedding",
    "bge",
    "nomic",
    "gte",
    "minilm",
    "e5",
    "instructor",
    "sentence",
    "similarity",
    "snowflake-arctic-embed",
    "jina-embeddings",
    "text-embedding",
    "clip",
    "siglip",
    "fashion-clip",
    "dinov2",
    "colpali",
    "vit",
    "swin",
];

pub const EMBEDDING_ARCHITECTURES: &[&str] = &[
    "bert",
    "roberta",
    "xlm-roberta",
    "modernbert",
    "deberta",
    "nomic-bert",
];

// ─── 4. Text-To-Speech Slot (/v1/audio/speech) ─────────────────────────
pub const AUDIO_TTS_KEYWORDS: &[&str] = &[
    "tts",
    "kokoro",
    "piper",
    "vits",
    "vocoder",
    "supertonic",
    "bark",
    "cosyvoice",
    "chattts",
    "parler",
    "fastspeech",
    "f5-tts",
    "e2-tts",
    "mms-tts",
];

// ─── 5. Speech-To-Text Slot (/v1/audio/transcriptions) ─────────────────
pub const AUDIO_ASR_KEYWORDS: &[&str] = &[
    "whisper",
    "asr",
    "stt",
    "conformer",
    "wav2vec",
    "hubert",
    "moonshine",
    "sensevoice",
];

pub const AUDIO_CONVERSION_KEYWORDS: &[&str] = &[
    "conversion",
    "demucs",
    "rvc",
    "sovits",
];

// ─── Helper GGUF Prefixes/Suffixes ─────────────────────────────────────
pub const HELPER_GGUF_PATTERNS: &[&str] = &[
    "mtp",
    "mmproj",
    "projector",
    "adapter",
    "lora",
];

// ─── Helper Detection Functions ────────────────────────────────────────

/// Utility function: Check if text contains any of the keywords
pub fn matches_any(text: &str, keywords: &[&str]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|&k| lower.contains(k))
}

/// Check if a model identifier represents an embedding model
pub fn is_embedding_ident(text: &str) -> bool {
    matches_any(text, EMBEDDING_KEYWORDS) || matches_any(text, EMBEDDING_ARCHITECTURES)
}

/// Check if a model identifier represents an Ingest OCR/Doc-AI model
pub fn is_ingest_ident(text: &str) -> bool {
    matches_any(text, INGEST_KEYWORDS)
}

/// Check if a model identifier represents a Multimodal VLM Chat model
pub fn is_vlm_chat_ident(text: &str) -> bool {
    matches_any(text, MULTIMODAL_CHAT_KEYWORDS)
}

/// Check if a model identifier represents a TTS model
pub fn is_tts_ident(text: &str) -> bool {
    matches_any(text, AUDIO_TTS_KEYWORDS)
}

/// Check if a model identifier represents an ASR model
pub fn is_asr_ident(text: &str) -> bool {
    matches_any(text, AUDIO_ASR_KEYWORDS)
}
