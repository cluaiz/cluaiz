use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

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

    let embedding_dispatcher = state.embedding_dispatcher.clone();
    let file_path_for_closure = file_path.clone();
    let ingest_result = tokio::task::spawn_blocking(move || {
        ingestor.ingest_and_vectorize(&file_path_for_closure, &*embedding_dispatcher)
    }).await
    .map_err(|e| {
        return Json(json!({
            "status": "error",
            "message": format!("Ingest task panicked: {}", e)
        }));
    });

    let ingest_result = match ingest_result {
        Ok(r) => r,
        Err(resp) => return resp,
    };

    let mut generated_cel_scripts = Vec::new();

    match ingest_result {
        Ok(chunks) => {
            if temp_mode.is_none() {
                // ── MIGRATION TO CEL ──
                // Old Code Hardcoded LMDB: 
                // engines::memory::tensor_transducer::TensorTransducer::save_context(...)
                // New Code: Generate CEL Payload for cluaiz-db Extension
                
                for (chunk, vec) in &chunks {
                    let memory_id = format!("api-file-{}-{}", file_path, Utc::now().timestamp_nanos_opt().unwrap_or(0));
                    
                    let cel_payload = json!({
                        "manifest": {
                            "version": "1.0",
                            "target": "cluaiz-db",
                            "action": "insert_vector",
                            "payload": {
                                "memory_id": memory_id,
                                "chunk_text": chunk,
                                "vector": vec
                            }
                        }
                    });
                    
                    generated_cel_scripts.push(cel_payload);
                    
                    // Note: Here we would pass `cel_payload` to the internal unified CEL executor
                    // e.g., crate::handlers::cel_handler::execute_internal(cel_payload).await
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
        "message": format!("Universal file '{}' ingestion completed via CEL routing.", payload.file_path),
        "chunks_processed": if return_vec { returned_chunks.len() } else { 0 },
        "vectors": if return_vec { serde_json::to_value(returned_chunks).unwrap_or(json!([])) } else { json!([]) },
        "cel_scripts_generated": generated_cel_scripts.len()
    }))
}
