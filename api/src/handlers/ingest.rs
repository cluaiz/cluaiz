use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;
use engines::neural_foundry::security::permission_schema::PermissionSchema;
use axum::response::IntoResponse;

use crate::handlers::chat::TemporaryChatMode;
use engines::neural_foundry::ingestion::DocumentIngestor;
use chrono::Utc;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct OutputControls {
    pub return_text: Option<bool>,
    pub return_embeddings: Option<bool>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ChunkingStrategy {
    pub r#type: Option<String>,
    pub max_chunk_size: Option<usize>,
    pub overlap: Option<usize>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct VisionSettings {
    pub use_vision: Option<bool>,
    pub detail_level: Option<String>,
    pub system_instruction: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum SourceInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct IngestPayload {
    pub source: SourceInput,
    pub namespace: Option<String>,
    pub model: Option<String>,
    pub output_controls: Option<OutputControls>,
    pub chunking_strategy: Option<ChunkingStrategy>,
    pub vision_settings: Option<VisionSettings>,
}

// ─── POST /v1/ingest/file ─────────────────────────────────────────
pub async fn file_ingest(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IngestPayload>
) -> axum::response::Response {
    // Default to true for embeddings output unless specified otherwise
    let return_vec = payload.output_controls
        .as_ref()
        .and_then(|c| c.return_embeddings)
        .unwrap_or(true);
        
    let chunk_size = payload.chunking_strategy
        .as_ref()
        .and_then(|cs| cs.max_chunk_size)
        .unwrap_or(512);

    let model_id = payload.model.clone();

    // Collect all sources
    let mut all_sources = Vec::new();
    match &payload.source {
        SourceInput::Single(s) => all_sources.push(s.clone()),
        SourceInput::Multiple(srcs) => all_sources.extend(srcs.clone()),
    }

    if all_sources.is_empty() {
        return Json(json!({
            "status": "error",
            "message": "You must provide either 'source' (string) or 'sources' (array of strings)."
        })).into_response();
    }

    let mut total_chunks_processed = 0;
    let mut data_array = Vec::new();
    let mut errors = Vec::new();

    let schema = PermissionSchema::load();
    if return_vec {
        if let Err(err_response) = crate::utils::slots::require_capability(
            &schema, 
            "embed_slot", 
            &["embedding"]
        ) {
            tracing::error!("Blocked ingest request: Active slot 'embed_slot' does not support embeddings.");
            return err_response.into_response();
        }
    }

    let supported_tasks = schema.active_slots.get("embed_slot")
        .map(|s| s.supported_tasks.clone())
        .unwrap_or_default();

    // Process each source sequentially for safety, though they could be spawned concurrently
    for source_url in all_sources.iter() {
        let ingestor = DocumentIngestor::new();
        let local_file_path = match crate::url_resolver::resolve_to_local_file(source_url).await {
            Ok(path) => path,
            Err(e) => {
                errors.push(format!("Failed to resolve source '{}': {}", source_url, e));
                continue;
            }
        };

        let embedding_dispatcher = state.embedding_dispatcher.clone();
        let file_path_for_closure = local_file_path.clone();
        let model_id_clone = model_id.clone();
        let vision_settings_clone = payload.vision_settings.clone();
        let supported_tasks_clone = supported_tasks.clone();
        
        let ingest_result = tokio::task::spawn_blocking(move || {
            ingestor.ingest_and_vectorize(&file_path_for_closure, &*embedding_dispatcher, model_id_clone, chunk_size, vision_settings_clone.and_then(|v| v.system_instruction), return_vec, &supported_tasks_clone)
        }).await;

        match ingest_result {
            Ok(Ok(chunks)) => {
                total_chunks_processed += chunks.len();
                
                let mut chunk_json_list = Vec::new();
                let return_text = payload.output_controls.as_ref().and_then(|c| c.return_text).unwrap_or(true);
                
                for (idx, (text, vector)) in chunks.into_iter().enumerate() {
                    let mut chunk_obj = serde_json::Map::new();
                    chunk_obj.insert("index".to_string(), json!(idx));
                    
                    if return_text {
                        chunk_obj.insert("text".to_string(), json!(text));
                    }
                    
                    if return_vec {
                        chunk_obj.insert("embedding".to_string(), json!(vector));
                    }
                    
                    chunk_json_list.push(serde_json::Value::Object(chunk_obj));
                }

                data_array.push(json!({
                    "source_url": source_url,
                    "total_file_chunks": chunk_json_list.len(),
                    "chunks": chunk_json_list
                }));
            },
            Ok(Err(e)) => {
                errors.push(format!("Ingestion failed for '{}': {}", source_url, e));
            },
            Err(e) => {
                errors.push(format!("Ingest task panicked for '{}': {}", source_url, e));
            }
        }
    }

    Json(json!({
        "object": "list",
        "namespace": payload.namespace.clone().unwrap_or_else(|| "default".to_string()),
        "data": data_array,
        "usage": {
            "total_files_processed": all_sources.len() - errors.len(),
            "total_chunks": total_chunks_processed,
            "prompt_tokens": 0 // Assuming token counting is added later
        },
        "errors": if errors.is_empty() { serde_json::Value::Null } else { json!(errors) }
    })).into_response()
}
