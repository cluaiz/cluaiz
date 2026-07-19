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

    let model_name = payload.model.unwrap_or_else(|| "onnx-embedding".to_string());
    let dispatcher = state.embedding_dispatcher.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut data_list = Vec::new();
        let mut total_tokens = 0;

        for (idx, text) in inputs.into_iter().enumerate() {
            total_tokens += text.split_whitespace().count();
            match dispatcher.dispatch_embedding(&text) {
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
                model: model_name,
                usage: EmbeddingUsage {
                    prompt_tokens: total_tokens,
                    total_tokens,
                },
            })),
        ),
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
        ),
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
        ),
    }
}
