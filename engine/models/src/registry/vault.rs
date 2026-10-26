//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Model Vault Directories (one folder per model category)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::PathBuf;

pub struct ModelVault;

impl ModelVault {
    /// Returns the root models directory (e.g. ~/.cluaiz/models)
    pub fn root_dir() -> PathBuf {
        cluaiz_core::EnvironmentManager::current()
            .ensure_models_dir()
            .unwrap_or_else(|_| cluaiz_core::EnvironmentManager::current().models_dir())
    }

    // ─── 1. Chat Category ───────────────────────────────────────────────
    pub fn chat_dir() -> PathBuf {
        Self::root_dir().join("chat")
    }

    // ─── 2. Ingest Category ─────────────────────────────────────────────
    pub fn ingest_dir() -> PathBuf {
        Self::root_dir().join("ingest")
    }

    // ─── 3. Embedding Category (Unified Text & Vision) ───────────────────
    pub fn embedding_dir() -> PathBuf {
        Self::root_dir().join("embedding")
    }

    // ─── 4. Text-To-Speech Category ─────────────────────────────────────
    pub fn tts_dir() -> PathBuf {
        Self::root_dir().join("tts")
    }

    // ─── 5. Speech-To-Text Category ─────────────────────────────────────
    pub fn stt_dir() -> PathBuf {
        Self::root_dir().join("stt")
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
}
