//! ═══════════════════════════════════════════════════════════════════════
//!   Taxonomy: Heuristic Matchers & Architectural Keyword Matrices (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

/// Text Embedding keyword heuristics
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
];

/// Dedicated Embedding Architectures (tensors, encoder-based, etc.)
pub const EMBEDDING_ARCHITECTURES: &[&str] = &[
    "bert",
    "roberta",
    "xlm-roberta",
    "modernbert",
    "deberta",
    "nomic-bert",
];

/// Text-to-Speech (TTS) keywords
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

/// Speech-to-Text (ASR) keywords
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

/// Voice Conversion / Separation keywords
pub const AUDIO_CONVERSION_KEYWORDS: &[&str] = &[
    "conversion",
    "demucs",
    "rvc",
    "sovits",
];

/// Vision & Multimodal keywords
pub const VISION_KEYWORDS: &[&str] = &[
    "clip",
    "siglip",
    "vision",
    "fashion-clip",
    "minicpm-v",
    "llava",
    "qwen-vl",
    "qwen2-vl",
    "internvl",
    "phi-3-vision",
    "paligemma",
    "florence",
    "vit",
    "swin",
    "dinov2",
];

/// Chat / Conversational keywords
pub const CHAT_KEYWORDS: &[&str] = &[
    "instruct",
    "chat",
    "-it",
    "dialogue",
    "assistant",
    "conversational",
];

/// Helper GGUF prefixes/suffixes to differentiate from primary weights
pub const HELPER_GGUF_PATTERNS: &[&str] = &[
    "mtp",
    "mmproj",
    "projector",
    "adapter",
    "lora",
    "vision",
];

/// Utility function: Check if text contains any of the keywords
pub fn matches_any(text: &str, keywords: &[&str]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|&k| lower.contains(k))
}

/// Check if a model identifier represents an embedding model
pub fn is_embedding_ident(text: &str) -> bool {
    matches_any(text, EMBEDDING_KEYWORDS) || matches_any(text, EMBEDDING_ARCHITECTURES)
}

/// Check if a model identifier represents a TTS model
pub fn is_tts_ident(text: &str) -> bool {
    matches_any(text, AUDIO_TTS_KEYWORDS)
}

/// Check if a model identifier represents an ASR model
pub fn is_asr_ident(text: &str) -> bool {
    matches_any(text, AUDIO_ASR_KEYWORDS)
}

/// Check if a model identifier represents a Vision model
pub fn is_vision_ident(text: &str) -> bool {
    matches_any(text, VISION_KEYWORDS)
}
