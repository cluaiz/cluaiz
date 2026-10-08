use axum::{Json, extract::State, response::IntoResponse, http::StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use crate::state::AppState;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Deserialize, Debug)]
pub struct EmbeddingRequest {
    pub input: EmbeddingInput,
    pub model: Option<String>,
    pub encoding_format: Option<String>,
    pub user: Option<String>,
}

#[derive(Serialize)]
pub struct EmbeddingData {
    pub object: &'static str,
    pub embedding: Vec<f32>,
    pub index: usize,
}

#[derive(Serialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Serialize)]
pub struct EmbeddingResponse {
    pub object: &'static str,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

// ─── POST /v1/embeddings ─────────────────────────────────────────
pub async fn generate_embeddings(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<EmbeddingRequest>,
) -> impl IntoResponse {
    let inputs = match payload.input {
        EmbeddingInput::Single(s) => vec![s],
        EmbeddingInput::Multiple(vec) => vec,
    };

    let schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    let target_slot = "embed_slot";

    if let Err(err_response) = crate::utils::slots::require_capability(
        &schema, 
        target_slot, 
        &["embedding", "feature-extraction", "vision-embedding", "audio-embedding"]
    ) {
        tracing::error!("Blocked embeddings request: Active slot '{}' does not support embedding tasks.", target_slot);
        return err_response.into_response();
    }

    let mut resolved_model_name = String::new();
    let mut active_model_path = None;

    // 1. Agar model explicitly diya hai payload mein (jo model diya usi model ko load karta hai)
    if let Some(req_model) = &payload.model {
        if !req_model.trim().is_empty() {
            if let Some(explicit_path) = crate::utils::slots::resolve_model_by_id(req_model) {
                active_model_path = Some(explicit_path);
                resolved_model_name = req_model.clone();
                tracing::info!("🤖 [API] Embeddings model override requested: '{}'", resolved_model_name);
            }
        }
    }

    // 2. Agar payload mein model nahi mila, toh default settings se uthao (jo setting mein set hai wo pick karta hai)
    if resolved_model_name.is_empty() {
        if let Some(slot) = schema.active_slots.get(target_slot) {
            if let Some(m_id) = &slot.model_id {
                if !m_id.trim().is_empty() {
                    resolved_model_name = m_id.clone();
                    active_model_path = crate::utils::slots::resolve_model_path(&schema, target_slot);
                    tracing::info!("🤖 [API] No model provided, falling back to slot setting: '{}'", resolved_model_name);
                }
            }
        }
    }

    // 3. Agar setting mein bhi kuch nahi hai, toh seedha error (model not defined)
    if resolved_model_name.is_empty() || active_model_path.is_none() {
        let err_res = json!({
            "error": {
                "message": "Model not defined. Please specify a model in the request payload or configure a default embedding model in the settings.",
                "type": "invalid_request_error",
                "code": "model_not_found"
            }
        });
        return axum::response::Json(err_res).into_response();
    }

    let dispatcher = state.embedding_dispatcher.clone();
    let target_path = active_model_path.clone().unwrap();

    let result = tokio::task::spawn_blocking(move || {
        let mut data_list = Vec::new();
        let mut total_tokens = 0;

        for (idx, text) in inputs.into_iter().enumerate() {
            total_tokens += text.split_whitespace().count();
            match dispatcher.dispatch_embedding_with_model(&text, &target_path) {
                Ok(vec) => {
                    data_list.push(EmbeddingData {
                        object: "embedding",
                        embedding: vec,
                        index: idx,
                    });
                }
                Err(e) => {
                    return Err(format!("Embedding generation failed for item {}: {:?}", idx, e));
                }
            }
        }

        Ok((data_list, total_tokens))
    })
    .await;

    match result {
        Ok(Ok((data_list, total_tokens))) => (
            StatusCode::OK,
            Json(json!(EmbeddingResponse {
                object: "list",
                data: data_list,
                model: resolved_model_name,
                usage: EmbeddingUsage {
                    prompt_tokens: total_tokens,
                    total_tokens,
                },
            })),
        ).into_response(),
        Ok(Err(err_msg)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {
                    "message": err_msg,
                    "type": "server_error",
                    "param": null,
                    "code": null
                }
            })),
        ).into_response(),
        Err(join_err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": {
                    "message": format!("Task execution error: {}", join_err),
                    "type": "server_error",
                    "param": null,
                    "code": null
                }
            })),
        ).into_response(),
    }
}
