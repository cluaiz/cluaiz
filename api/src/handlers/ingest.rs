//! 📥 Ingestion & Document Processing Handler (`api/src/handlers/ingest.rs`)

use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use crate::AppState;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OutputControls {
    pub return_text: Option<bool>,
    pub return_embeddings: Option<bool>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ChunkingStrategy {
    pub r#type: Option<String>,
    pub max_chunk_size: Option<usize>,
    pub overlap: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct VisionSettings {
    pub use_vision: Option<bool>,
    pub detail_level: Option<String>,
    pub system_instruction: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum SourceInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize, Debug, Clone)]
pub struct IngestPayload {
    pub source: SourceInput,
    pub namespace: Option<String>,
    pub model: Option<String>,
    pub output_controls: Option<OutputControls>,
    pub chunking_strategy: Option<ChunkingStrategy>,
    pub vision_settings: Option<VisionSettings>,
}

/// POST /v1/ingest & POST /v1/ingest/file
pub async fn file_ingest(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<IngestPayload>,
) -> impl IntoResponse {
    let mut all_sources = Vec::new();
    match &payload.source {
        SourceInput::Single(s) => all_sources.push(s.clone()),
        SourceInput::Multiple(srcs) => all_sources.extend(srcs.clone()),
    }

    if all_sources.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "You must provide either 'source' (string) or 'sources' (array of strings)."
        }));
    }

    let chunk_size = payload.chunking_strategy
        .as_ref()
        .and_then(|cs| cs.max_chunk_size)
        .unwrap_or(512);

    let return_text = payload.output_controls
        .as_ref()
        .and_then(|c| c.return_text)
        .unwrap_or(true);

    let mut processed = Vec::new();
    for src in all_sources {
        let local_path = match crate::url_resolver::resolve_to_local_file(&src).await {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => std::path::PathBuf::from(&src),
        };

        if !local_path.exists() {
            processed.push(json!({
                "source": src,
                "status": "error",
                "error": format!("File not found: {}", src)
            }));
            continue;
        }

        let ext = local_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match std::fs::read_to_string(&local_path) {
            Ok(content) => {
                let chunks = cluaiz_models::SemanticChunker::chunk(&content, ext, chunk_size);
                let mut item = json!({
                    "source": src,
                    "status": "ingested",
                    "chunks_count": chunks.len(),
                    "chunk_size": chunk_size
                });
                if return_text {
                    item["chunks"] = json!(chunks);
                }
                processed.push(item);
            }
            Err(e) => {
                processed.push(json!({
                    "source": src,
                    "status": "error",
                    "error": format!("Failed to read file: {}", e)
                }));
            }
        }
    }

    Json(json!({
        "status": "success",
        "processed_sources": processed,
        "total_sources": processed.len()
    }))
}
