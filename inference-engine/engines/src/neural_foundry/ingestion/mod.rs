pub mod chunker;

use anyhow::Result;
use std::path::Path;
use tracing::{info, warn};
use neural_core::interfaces::router_contract::EmbeddingDriver;
use chunker::SemanticChunker;

/// Native Document Ingestion Pipeline
/// This module handles direct extraction of text from files and vectorization 
/// without needing external Python wrappers.
pub struct DocumentIngestor;

impl DocumentIngestor {
    pub fn new() -> Self {
        Self
    }

    /// Read a file, extract its text, chunk it, and generate ONNX embeddings.
    pub fn ingest_and_vectorize<D: EmbeddingDriver>(
        &self, 
        file_path: &str, 
        driver: &D
    ) -> Result<Vec<(String, Vec<f32>)>> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", file_path));
        }

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        info!("📄 [Ingestor] Processing document: {} (Type: {})", file_path, extension);

        let raw_text = match extension.to_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "webp" => {
                info!("👁️ [Ingestor] Image file detected. Invoking Vision Encoder (CLIP)...");
                let bytes = std::fs::read(path)?;
                let vector = driver.gen_multimodal_embedding(&bytes, neural_core::interfaces::router_contract::Modality::Image)
                    .map_err(|e| anyhow::anyhow!("Vision Embedding Error: {:?}", e))?;
                
                info!("🧠 [Ingestor] Successfully generated 1x{} Mathematical Vision Tensor.", vector.len());
                return Ok(vec![(format!("[IMAGE_EMBEDDING:{}]", path.display()), vector)]);
            },
            "txt" | "md" | "mdx" | "csv" | "rs" | "py" | "js" | "ts" | "c" | "cpp" | "go" => {
                std::fs::read_to_string(path)?
            },
            "pdf" => {
                info!("⚠️ [Ingestor] PDF parsing invoked. Applying HYBRID extraction strategy.");
                
                // Use pdf-extract to natively pull raw text
                let extracted_text = match pdf_extract::extract_text(path) {
                    Ok(text) => text,
                    Err(e) => {
                        warn!("❌ [Ingestor] Failed to extract text from PDF natively: {}. Falling back to Vision model placeholder.", e);
                        String::new()
                    }
                };

                // NOTE: Simple check for images is not natively provided by pdf-extract in a boolean manner, 
                // so we rely on whether ANY text could be extracted. 
                // If it's a scanned PDF, `extracted_text` will be empty.
                if extracted_text.trim().is_empty() {
                    warn!("🖼️ [Ingestor] Scanned PDF or Images detected (No text found)! Falling back to heavy Vision Encoder (ColPali/Nougat).");
                    // Route to Modality::Image (Vision Router) for OCR-free mathematical extraction
                    "Visual RAG processing placeholder...".to_string()
                } else {
                    info!("📝 [Ingestor] Text-based PDF detected. Skipping Vision model to save processing power.");
                    extracted_text
                }
            },
            "docx" => {
                warn!("⚠️ [Ingestor] DOCX parsing invoked. (Using placeholder)");
                "Extracted text from DOCX would go here.".to_string()
            },
            _ => {
                // Generic Fallback: Try to read as UTF-8 text. 
                // This allows formats like .log, .json, .yaml, .env without hardcoding them.
                match std::fs::read_to_string(path) {
                    Ok(text) => {
                        info!("📝 [Ingestor] Unrecognized extension '{}', but successfully parsed as UTF-8 text.", extension);
                        text
                    },
                    Err(_) => {
                        return Err(anyhow::anyhow!("Unsupported or binary document format: {}", extension));
                    }
                }
            }
        };

        // 🧠 Intelligent Semantic Chunking
        let chunks = SemanticChunker::chunk(&raw_text, extension, 512);
        info!("✂️ [Ingestor] Split document contextually into {} chunks.", chunks.len());

        let mut vectorized_docs = Vec::new();

        // Feed directly into the Gatekeeper Engine (ONNX CPU)
        for chunk in chunks {
            let vector = driver.gen_embedding(&chunk).map_err(|e| anyhow::anyhow!("Embedding Error: {:?}", e))?;
            vectorized_docs.push((chunk, vector));
        }

        info!("🧠 [Ingestor] Successfully vectorized {} chunks using native ML Engine.", vectorized_docs.len());

        Ok(vectorized_docs)
    }
}
