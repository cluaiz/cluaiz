use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

use crate::handlers::chat::TemporaryChatMode;
use engines::neural_foundry::ingestion::DocumentIngestor;
use chrono::Utc;

#[derive(serde::Deserialize)]
pub struct IngestPayload {
    pub file_path: String,
    pub temporary_chat: Option<TemporaryChatMode>,
    pub return_vectors: Option<bool>,
}

// ─── POST /v1/ingest/file ─────────────────────────────────────────
pub async fn file_ingest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IngestPayload>
) -> Json<Value> {
    let file_path = payload.file_path.clone();
    let temp_mode = payload.temporary_chat.clone();
    let return_vec = payload.return_vectors.unwrap_or(false);

    let ingestor = DocumentIngestor::new();
    let mut returned_chunks = Vec::new();

    // Foregound Processing for API response
    match ingestor.ingest_and_vectorize(&file_path, &*state.embedding_dispatcher) {
        Ok(chunks) => {
            if temp_mode.is_none() {
                // Save to LMDB
                for (chunk, vec) in &chunks {
                    let memory_id = format!("api-file-{}-{}", file_path, Utc::now().timestamp_nanos_opt().unwrap_or(0));
                    let _ = engines::memory::tensor_transducer::TensorTransducer::save_context(&memory_id, chunk, vec);
                }
            }

            if return_vec {
                returned_chunks = chunks;
            }
        },
        Err(e) => {
            return Json(json!({
                "status": "error",
                "message": format!("Ingestion failed: {}", e)
            }));
        }
    }

    Json(json!({
        "status": "success", 
        "message": format!("Universal file '{}' ingestion completed.", payload.file_path),
        "chunks_processed": if return_vec { returned_chunks.len() } else { 0 },
        "vectors": if return_vec { serde_json::to_value(returned_chunks).unwrap_or(json!([])) } else { json!([]) }
    }))
}
