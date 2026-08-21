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

    // ─── 2. Ingest Category ─────────────────────────────────────────────
    pub fn ingest_dir() -> PathBuf {
        Self::root_dir().join("ingest")
    }

    pub fn ingest_endpoint() -> &'static str {
        "/v1/ingest"
    }

    // ─── 3. Embedding Category (Unified Text & Vision) ───────────────────
    pub fn embedding_dir() -> PathBuf {
        Self::root_dir().join("embedding")
    }

    pub fn embedding_endpoint() -> &'static str {
        "/v1/embeddings"
    }

    // ─── 4. Text-To-Speech Category ─────────────────────────────────────
    pub fn tts_dir() -> PathBuf {
        Self::root_dir().join("tts")
    }

    pub fn tts_endpoint() -> &'static str {
        "/v1/audio/speech"
    }

    // ─── 5. Speech-To-Text Category ─────────────────────────────────────
    pub fn stt_dir() -> PathBuf {
        Self::root_dir().join("stt")
    }

    pub fn stt_endpoint() -> &'static str {
        "/v1/audio/transcriptions"
    }

    /// Returns all 5 sovereign category descriptors with their folders and API endpoints
    pub fn category_descriptors() -> Vec<CategoryDescriptor> {
        vec![
            CategoryDescriptor {
                category: "chat",
                path: Self::chat_dir(),
                endpoint: Self::chat_endpoint(),
                description: "Multi-Turn Dialogue LLMs & Multimodal Chat VLMs (Qwen2.5, Qwen2-VL, Llama 3.3)",
            },
            CategoryDescriptor {
                category: "ingest",
                path: Self::ingest_dir(),
                endpoint: Self::ingest_endpoint(),
                description: "Document OCR, Tables, SAM & Spatial Vision (GOT-OCR, Nougat, Florence-2)",
            },
            CategoryDescriptor {
                category: "embedding",
                path: Self::embedding_dir(),
                endpoint: Self::embedding_endpoint(),
                description: "Unified Multimodal Vector Embeddings (BGE, Nomic, CLIP, SigLIP, ColPali)",
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
                description: "Speech-to-Text Audio Transcription (Whisper, Moonshine, SenseVoice)",
            },
        ]
    }

    /// Returns category name and directory path pairs for scanning
    pub fn category_dirs() -> Vec<(&'static str, PathBuf)> {
        vec![
            ("chat", Self::chat_dir()),
            ("ingest", Self::ingest_dir()),
            ("embedding", Self::embedding_dir()),
            ("tts", Self::tts_dir()),
            ("stt", Self::stt_dir()),
        ]
    }

    /// Resolves canonical category folder path strictly from category name
    pub fn resolve_category_dir(category: &str) -> PathBuf {
        let cat_lower = category.to_lowercase().replace('_', "-");
        match cat_lower.as_str() {
            "chat" => Self::chat_dir(),
            "ingest" => Self::ingest_dir(),
            "embedding" => Self::embedding_dir(),
            "tts" => Self::tts_dir(),
            "stt" => Self::stt_dir(),
            _ => Self::chat_dir(),
        }
    }

    /// Resolves canonical API endpoint strictly from category name
    pub fn resolve_category_endpoint(category: &str) -> &'static str {
        let cat_lower = category.to_lowercase().replace('_', "-");
        match cat_lower.as_str() {
            "chat" => Self::chat_endpoint(),
            "ingest" => Self::ingest_endpoint(),
            "embedding" => Self::embedding_endpoint(),
            "tts" => Self::tts_endpoint(),
            "stt" => Self::stt_endpoint(),
            _ => Self::chat_endpoint(),
        }
    }
}
