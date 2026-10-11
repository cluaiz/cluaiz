pub mod chunker;

use anyhow::Result;
use std::path::Path;
use tracing::{info, warn};
use cluaiz_core::EmbeddingDriver;
pub use chunker::SemanticChunker;

/// Native Document Ingestion Pipeline
/// Handles direct extraction of text from files and vectorization.
pub struct DocumentIngestor;

impl DocumentIngestor {
    pub fn new() -> Self {
        Self
    }

    /// Read a file, extract its text, chunk it, and generate embeddings.
    pub fn ingest_and_vectorize<D: EmbeddingDriver>(
        &self, 
        file_path: &str, 
        driver: &D,
        model_id: Option<String>,
        chunk_size: usize,
        vision_settings_instruction: Option<String>,
        generate_embeddings: bool,
        supported_tasks: &[String]
    ) -> Result<Vec<(String, Vec<f32>)>> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_path));
        }

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        info!("📄 [Ingestor] Processing document: {} (Type: {})", file_path, extension);

        if let Some(ref m) = model_id {
            info!("🤖 [Ingestor] Explicit model override requested: '{}'.", m);
        }

        let raw_text = match extension.to_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "webp" => {
                if !supported_tasks.iter().any(|t| t == "vision" || t == "image-to-text" || t == "multimodal") {
                    info!("ℹ️ [Ingestor] Processing image file '{}' (supported_tasks: {:?}).", extension, supported_tasks);
                }
                
                info!("👁️ [Ingestor] Image file detected. Invoking Vision Encoder...");
                let opt_instruction = vision_settings_instruction;

                if !generate_embeddings {
                    return Ok(vec![(format!("[IMAGE:{}]", path.display()), vec![])]);
                }

                let bytes = std::fs::read(path)?;
                let vector = driver.gen_multimodal_embedding(&bytes, cluaiz_core::Modality::Image, opt_instruction)
                    .map_err(|e| anyhow::anyhow!("Model embedding failed: {:?}", e))?;
                
                info!("🧠 [Ingestor] Successfully generated 1x{} Vision Vector.", vector.len());
                return Ok(vec![(format!("[IMAGE_EMBEDDING:{}]", path.display()), vector)]);
            },
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => {
                return Err(anyhow::anyhow!("Native audio transcription is not implemented yet for '{}'. Ingestion currently supports text, code, markdown, and image modalities.", file_path));
            },
            _ => {
                match std::fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(_) => {
                        return Err(anyhow::anyhow!("Unsupported or binary document format: {}", extension));
                    }
                }
            }
        };

        let chunks = SemanticChunker::chunk(&raw_text, extension, chunk_size);
        info!("✂️ [Ingestor] Split document contextually into {} chunks (max size: {}).", chunks.len(), chunk_size);

        let mut vectorized_docs = Vec::new();
        for chunk in chunks {
            if generate_embeddings {
                let vector = driver.gen_embedding(&chunk)
                    .map_err(|e| anyhow::anyhow!("Embedding generation failed: {:?}", e))?;
                vectorized_docs.push((chunk, vector));
            } else {
                vectorized_docs.push((chunk, vec![]));
            }
        }

        Ok(vectorized_docs)
    }
}
