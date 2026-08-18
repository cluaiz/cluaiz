//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Model Vault Directory & API Endpoint Registry (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CategoryDescriptor {
    pub category: &'static str,
    pub path: PathBuf,
    pub endpoint: &'static str,
    pub description: &'static str,
}

pub struct ModelVault;

impl ModelVault {
    /// Returns the root models directory (e.g. ~/.cluaiz/models)
    pub fn root_dir() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current()
            .ensure_models_dir()
            .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir())
    }

    // ─── 1. Chat Category ───────────────────────────────────────────────
    pub fn chat_dir() -> PathBuf {
        Self::root_dir().join("chat")
    }

    pub fn chat_endpoint() -> &'static str {
        "/v1/chat/completions"
    }

    // ─── 2. Vision Ingest Category ──────────────────────────────────────
    pub fn vision_ingest_dir() -> PathBuf {
        Self::root_dir().join("vision-ingest")
    }

    pub fn vision_ingest_endpoint() -> &'static str {
        "/v1/ingest/file"
    }

    // ─── 3. Vision Embedding Category ───────────────────────────────────
    pub fn vision_embedding_dir() -> PathBuf {
        Self::root_dir().join("vision-embedding")
    }

    pub fn vision_embedding_endpoint() -> &'static str {
        "/v1/embeddings/vision"
    }

    // ─── 4. Text Embedding Category ─────────────────────────────────────
    pub fn text_embedding_dir() -> PathBuf {
        Self::root_dir().join("text-embedding")
    }

    pub fn text_embedding_endpoint() -> &'static str {
        "/v1/embeddings"
    }

    // ─── 5. Text-To-Speech Category ─────────────────────────────────────
    pub fn tts_dir() -> PathBuf {
        Self::root_dir().join("tts")
    }

    pub fn tts_endpoint() -> &'static str {
        "/v1/audio/speech"
    }

    // ─── 6. Speech-To-Text Category ─────────────────────────────────────
    pub fn stt_dir() -> PathBuf {
        Self::root_dir().join("stt")
    }

    pub fn stt_endpoint() -> &'static str {
        "/v1/audio/transcriptions"
    }

    /// Returns all 6 sovereign category descriptors with their folders and API endpoints
    pub fn category_descriptors() -> Vec<CategoryDescriptor> {
        vec![
            CategoryDescriptor {
                category: "chat",
                path: Self::chat_dir(),
                endpoint: Self::chat_endpoint(),
                description: "Pure Text & Multi-Turn Dialogue LLMs",
            },
            CategoryDescriptor {
                category: "vision-ingest",
                path: Self::vision_ingest_dir(),
                endpoint: Self::vision_ingest_endpoint(),
                description: "Document OCR & VLM Image-to-Text Parsers",
            },
            CategoryDescriptor {
                category: "vision-embedding",
                path: Self::vision_embedding_dir(),
                endpoint: Self::vision_embedding_endpoint(),
                description: "Vision Vector Embeddings (CLIP, SigLIP, ColPali)",
            },
            CategoryDescriptor {
                category: "text-embedding",
                path: Self::text_embedding_dir(),
                endpoint: Self::text_embedding_endpoint(),
                description: "Text Vector Embeddings & Rerankers (BGE, Nomic)",
            },
            CategoryDescriptor {
                category: "tts",
                path: Self::tts_dir(),
                endpoint: Self::tts_endpoint(),
                description: "Text-to-Speech Audio Generation (Kokoro, Piper)",
            },
            CategoryDescriptor {
                category: "stt",
                path: Self::stt_dir(),
                endpoint: Self::stt_endpoint(),
                description: "Speech-to-Text Audio Transcription (Whisper, SenseVoice)",
            },
        ]
    }

    /// Returns category name and directory path pairs for scanning
    pub fn category_dirs() -> Vec<(&'static str, PathBuf)> {
        vec![
            ("chat", Self::chat_dir()),
            ("vision-ingest", Self::vision_ingest_dir()),
            ("vision-embedding", Self::vision_embedding_dir()),
            ("text-embedding", Self::text_embedding_dir()),
            ("tts", Self::tts_dir()),
            ("stt", Self::stt_dir()),
        ]
    }

    /// Resolves canonical category folder path strictly from category name
    pub fn resolve_category_dir(category: &str) -> PathBuf {
        let cat_lower = category.to_lowercase().replace('_', "-");
        match cat_lower.as_str() {
            "chat" => Self::chat_dir(),
            "vision-ingest" => Self::vision_ingest_dir(),
            "vision-embedding" => Self::vision_embedding_dir(),
            "text-embedding" => Self::text_embedding_dir(),
            "tts" => Self::tts_dir(),
            "stt" => Self::stt_dir(),
            _ => Self::root_dir().join(cat_lower),
        }
    }

    /// Resolves canonical API endpoint strictly from category name
    pub fn resolve_category_endpoint(category: &str) -> &'static str {
        let cat_lower = category.to_lowercase().replace('_', "-");
        match cat_lower.as_str() {
            "chat" => Self::chat_endpoint(),
            "vision-ingest" => Self::vision_ingest_endpoint(),
            "vision-embedding" => Self::vision_embedding_endpoint(),
            "text-embedding" => Self::text_embedding_endpoint(),
            "tts" => Self::tts_endpoint(),
            "stt" => Self::stt_endpoint(),
            _ => "/v1/chat/completions",
        }
    }
}
