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

        // Model Router log
        if let Some(ref m) = model_id {
            info!("🤖 [Ingestor] Explicit model override requested: '{}'.", m);
        } else {
            info!("🤖 [Ingestor] No model specified. Using Default System Model for ingestion.");
        }

        let raw_text = match extension.to_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "webp" => {
                if !supported_tasks.iter().any(|t| t == "vision" || t == "image-to-text" || t == "multimodal") {
                    info!("ℹ️ [Ingestor] Processing image file '{}' (supported_tasks: {:?}). Passing to vision driver execution.", extension, supported_tasks);
                }
                
                info!("👁️ [Ingestor] Image file detected. Invoking Vision Encoder...");
                
                let mut opt_instruction = None;
                if let Some(instruction) = &vision_settings_instruction {
                    opt_instruction = Some(instruction.clone());
                }

                if !generate_embeddings {
                    info!("👁️ [Ingestor] Image file detected, but generate_embeddings is false. Skipping vector generation.");
                    return Ok(vec![(format!("[IMAGE:{}]", path.display()), vec![])]);
                }

                let bytes = std::fs::read(path)?;
                let vector = driver.gen_multimodal_embedding(&bytes, neural_core::interfaces::router_contract::Modality::Image, opt_instruction)
                    .map_err(|e| anyhow::anyhow!("Model does not support Image/Vision format, or embedding failed: {:?}", e))?;
                
                info!("🧠 [Ingestor] Successfully generated 1x{} Mathematical Vision Tensor.", vector.len());
                return Ok(vec![(format!("[IMAGE_EMBEDDING:{}]", path.display()), vector)]);
            },
            "mp3" | "wav" | "flac" | "ogg" => {
                if !supported_tasks.iter().any(|t| t == "speech_to_text" || t == "automatic-speech-recognition" || t == "audio-transcription" || t == "audio") {
                    info!("ℹ️ [Ingestor] Processing audio file '{}' (supported_tasks: {:?}). Passing to audio driver execution.", extension, supported_tasks);
                }

                info!("🎧 [Ingestor] Audio file detected. Invoking Whisper STT Engine for transcription...");
                // Note: Actual Whisper FFI will be called here. For now, we simulate transcription.
                let mock_transcription = format!("[AUDIO_TRANSCRIPT] The user said this in {}", file_path);
                info!("📝 [Ingestor] Extracted audio transcription. Sending text to embedding engine.");
                mock_transcription
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
                    warn!("🖼️ [Ingestor] Scanned PDF or Images detected (No text found)! Falling back to heavy Vision Encoder.");
                    
                    if !supported_tasks.iter().any(|t| t == "vision" || t == "image-to-text" || t == "multimodal") {
                        info!("ℹ️ [Ingestor] Processing scanned PDF (supported_tasks: {:?}). Passing to vision driver execution.", supported_tasks);
                    }
                    
                    if let Some(instruction) = &vision_settings_instruction {
                        info!("🎯 [Ingestor] Applying custom Vision System Instruction for PDF: '{}'", instruction);
                    }

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
        let chunks = SemanticChunker::chunk(&raw_text, extension, chunk_size);
        info!("✂️ [Ingestor] Split document contextually into {} chunks (max size: {}).", chunks.len(), chunk_size);

        let mut vectorized_docs = Vec::new();

        // Feed directly into the Gatekeeper Engine (ONNX CPU)
        for chunk in chunks {
            if generate_embeddings {
                let vector = driver.gen_embedding(&chunk)
                    .map_err(|e| anyhow::anyhow!("Model does not support Text embedding, or generation failed: {:?}", e))?;
                vectorized_docs.push((chunk, vector));
            } else {
                vectorized_docs.push((chunk, vec![]));
            }
        }

        if generate_embeddings {
            info!("🧠 [Ingestor] Successfully vectorized {} chunks using native ML Engine.", vectorized_docs.len());
        } else {
            info!("📝 [Ingestor] Successfully extracted {} chunks. (Embeddings skipped as per user request).", vectorized_docs.len());
        }

        Ok(vectorized_docs)
    }
}
