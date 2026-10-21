//! Shared document extraction API. Filesystem policy and source decoding have one owner.

use super::ingest_model::{document_item, ModelFailure, Processor};
use crate::{
    utils::{authorize_source, task_error},
    AppState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use cluaiz_media::{
    documents::{self, DEFAULT_CHUNK_BYTES},
    source::SourceResolver,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct OutputControls {
    pub return_text: Option<bool>,
    pub return_embeddings: Option<bool>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChunkingStrategy {
    pub r#type: Option<String>,
    /// Maximum UTF-8 bytes in a returned chunk, not model tokens.
    pub max_chunk_size: Option<usize>,
    pub overlap: Option<usize>,
}
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct VisionSettings {
    pub use_vision: Option<bool>,
    pub detail_level: Option<String>,
    pub system_instruction: Option<String>,
    pub max_tokens: Option<usize>,
}
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum SourceInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct IngestPayload {
    pub source: SourceInput,
    pub namespace: Option<String>,
    pub model: Option<String>,
    pub output_controls: Option<OutputControls>,
    pub chunking_strategy: Option<ChunkingStrategy>,
    pub vision_settings: Option<VisionSettings>,
}

/// Existing ingest routes now execute the same extraction service as chat.
pub async fn file_ingest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<IngestPayload>,
) -> Response {
    let processor = match Processor::select(&payload) {
        Ok(processor) => processor,
        Err(response) => return response,
    };
    let sources = match payload.source {
        SourceInput::Single(source) => vec![source],
        SourceInput::Multiple(sources) => sources,
    };
    let invalid =
        |message: &str| task_error(StatusCode::BAD_REQUEST, "invalid_request_error", message);
    if sources.is_empty() || sources.iter().any(|source| source.trim().is_empty()) {
        return invalid("At least one non-empty document source is required");
    }
    if payload.namespace.is_some() {
        return invalid("Persistent namespaces are not implemented by document extraction");
    }
    let strategy = payload.chunking_strategy.as_ref();
    if strategy.is_some_and(|s| {
        s.overlap.unwrap_or(0) != 0 || s.r#type.as_deref().is_some_and(|t| t != "semantic")
    }) {
        return invalid("Only semantic chunking with zero overlap is currently supported");
    }
    let chunk_size = strategy
        .and_then(|s| s.max_chunk_size)
        .unwrap_or(DEFAULT_CHUNK_BYTES);
    let settings = cluaiz_core::Settings::current();
    if chunk_size == 0
        || u64::try_from(chunk_size).map_or(true, |size| {
            size > settings.media.limits.max_decoded_bytes()
        })
    {
        return invalid("Chunk size must be positive and fit the document byte limit");
    }
    let return_text = payload
        .output_controls
        .as_ref()
        .and_then(|c| c.return_text)
        .unwrap_or(true);
    let resolver = match SourceResolver::from_environment(settings.media) {
        Ok(resolver) => resolver,
        Err(error) => return task_error(StatusCode::INTERNAL_SERVER_ERROR, "source_error", error),
    };
    let mut processed = Vec::new();
    let mut input_bytes = 0u64;
    let mut output_bytes = 0u64;
    for original in sources {
        let mut source = original.clone();
        let allowed = match authorize_source(&state, &headers, &mut source).await {
            Ok(allowed) => allowed,
            Err((kind, message)) => {
                return task_error(
                    if kind == "permission_error" {
                        StatusCode::FORBIDDEN
                    } else {
                        StatusCode::BAD_REQUEST
                    },
                    kind,
                    message,
                )
            }
        };
        let result = async {
            let resolved = resolver
                .resolve(&source, |path| {
                    if allowed.as_deref() == Some(path) {
                        Ok(())
                    } else {
                        anyhow::bail!("Document source was not authorized")
                    }
                })
                .await?;
            input_bytes = input_bytes
                .checked_add(resolved.input_bytes)
                .ok_or_else(|| anyhow::anyhow!("Document batch size overflow"))?;
            settings.media.limits.check_input_bytes(input_bytes)?;
            let bytes = resolved.read_bytes(settings.media.limits).await?;
            let format = resolved.format.clone();
            drop(resolved);
            let mut item = if let Some(processor) = &processor {
                processor
                    .process(
                        &state,
                        bytes,
                        format,
                        settings.clone(),
                        chunk_size,
                        return_text,
                    )
                    .await?
            } else {
                let limits = settings.media.limits;
                let document = tokio::task::spawn_blocking(move || {
                    documents::extract(&bytes, &format, limits)
                })
                .await??;
                document_item(document, limits, chunk_size, return_text)?
            };
            item["source"] = json!(original);
            Ok::<_, anyhow::Error>(item)
        }
        .await;
        let item = match result {
            Ok(item) => item,
            Err(error) => {
                let mut item =
                    json!({"source":original, "status":"error", "error":format!("{error:#}")});
                if let Some(failure) = error.downcast_ref::<ModelFailure>() {
                    item["http_status"] = json!(failure.status.as_u16());
                    item["model_error"] = failure.body.clone();
                }
                item
            }
        };
        let serialized = match serde_json::to_vec(&item) {
            Ok(bytes) => bytes,
            Err(error) => {
                return task_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "serialization_error",
                    error,
                )
            }
        };
        output_bytes = match output_bytes.checked_add(serialized.len() as u64) {
            Some(bytes) => bytes,
            None => return invalid("Document output size overflow"),
        };
        if settings
            .media
            .limits
            .check_decoded_bytes(output_bytes)
            .is_err()
        {
            return invalid("Document batch output exceeds the decoded byte limit");
        }
        processed.push(item);
    }
    let successes = processed
        .iter()
        .filter(|item| item["status"] == "ingested")
        .count();
    let status = if successes == processed.len() {
        "success"
    } else if processed.iter().all(|item| item["status"] == "error") {
        "error"
    } else {
        "partial"
    };
    let http = if status == "error" {
        processed
            .first()
            .and_then(|item| item["http_status"].as_u64())
            .and_then(|code| u16::try_from(code).ok())
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::BAD_REQUEST)
    } else {
        StatusCode::OK
    };
    (http, Json(json!({"status":status, "total_sources":processed.len(), "processed_sources":processed}))).into_response()
}
