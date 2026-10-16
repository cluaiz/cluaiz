//! Selected-model embeddings; no generated substitute vectors.
use crate::{utils::prepare_authorized_media, AppState};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use cluaiz_core::{
    media::{InputPart, PreparedPart},
    ChatEnvelope, ChatMessage,
};
use cluaiz_runtime::onnx_host::{EmbeddingInput as PreparedEmbedding, OnnxModel};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Multiple(Vec<String>),
    Parts(Vec<InputPart>),
}
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingRequest {
    pub input: EmbeddingInput,
    pub model: Option<String>,
    pub encoding_format: Option<String>,
    pub user: Option<String>,
    pub keep_alive: Option<i32>,
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
use crate::utils::task_error as error;
fn validate(payload: &EmbeddingRequest) -> anyhow::Result<()> {
    if payload
        .encoding_format
        .as_deref()
        .is_some_and(|value| value != "float")
    {
        anyhow::bail!("Only float embedding encoding is currently implemented");
    }
    if payload.keep_alive.is_some_and(|value| value < -1) {
        anyhow::bail!("Invalid keep_alive");
    }
    match &payload.input {
        EmbeddingInput::Single(text) if text.trim().is_empty() => {
            anyhow::bail!("Embedding text is empty")
        }
        EmbeddingInput::Multiple(texts)
            if texts.is_empty() || texts.iter().any(|s| s.trim().is_empty()) =>
        {
            anyhow::bail!("Embedding batch contains empty text")
        }
        EmbeddingInput::Parts(parts) => {
            if parts
                .iter()
                .filter(|p| matches!(p, InputPart::Image { .. }))
                .count()
                != 1
                || parts
                    .iter()
                    .any(|p| !matches!(p, InputPart::Text { .. } | InputPart::Image { .. }))
            {
                anyhow::bail!(
                    "A media embedding requires exactly one image and optional text parts"
                );
            }
        }
        _ => (),
    }
    Ok(())
}
/// POST /v1/embeddings: string/string batch, or one image with optional text parts.
pub async fn generate_embeddings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<EmbeddingRequest>,
) -> Response {
    if let Err(e) = validate(&payload) {
        return error(StatusCode::BAD_REQUEST, "invalid_request_error", e);
    }
    let settings = cluaiz_core::Settings::current();
    let configured = state
        .permission_schema
        .read()
        .await
        .get_active_embedding_model();
    let requested = payload
        .model
        .as_deref()
        .filter(|s| !s.trim().is_empty() && *s != "auto")
        .or(configured.as_deref());
    let Some(requested) = requested else {
        return error(
            StatusCode::BAD_REQUEST,
            "model_required",
            "Select an installed embedding model",
        );
    };
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    let entry = registry.installed_models.get(requested).or_else(|| {
        registry
            .installed_models
            .values()
            .find(|entry| entry.id.eq_ignore_ascii_case(requested))
    });
    let Some(entry) = entry else {
        return error(
            StatusCode::NOT_FOUND,
            "model_not_found",
            "Selected embedding model is not installed",
        );
    };
    if !entry.supported_tasks.iter().any(|t| {
        matches!(
            t.replace('-', "_").as_str(),
            "embedding"
                | "feature_extraction"
                | "sentence_similarity"
                | "vision_embedding"
                | "image_feature_extraction"
        )
    }) {
        return error(
            StatusCode::BAD_REQUEST,
            "unsupported_task",
            "Selected model does not declare an embedding task",
        );
    }
    let model = match OnnxModel::from_package(entry) {
        Ok(model) => model,
        Err(e) => return error(StatusCode::BAD_REQUEST, "invalid_model", e),
    };
    let id = model.id.clone();
    let inputs = match payload.input {
        EmbeddingInput::Single(text) => vec![PreparedEmbedding::Text(text)],
        EmbeddingInput::Multiple(texts) => texts.into_iter().map(PreparedEmbedding::Text).collect(),
        EmbeddingInput::Parts(parts) => {
            let request = ChatEnvelope {
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: parts,
                }],
                samplers: Default::default(),
                think_mode: "off".into(),
                reasoning_effort: Default::default(),
                reasoning_budget_message: None,
                chat_template_kwargs: None,
                grammar: None,
                response_format: None,
            };
            let request =
                match prepare_authorized_media(&state, &headers, request, &settings, None, None).await {
                    Ok(request) => request,
                    Err((kind, message)) => {
                        return error(
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
            let mut image = None;
            let mut text = Vec::new();
            for part in request.messages.into_iter().flat_map(|m| m.content) {
                match part {
                    PreparedPart::Image { image: value } => image = Some(value),
                    PreparedPart::Text { text: value } => text.push(value),
                    _ => (),
                }
            }
            let Some(image) = image else {
                return error(
                    StatusCode::BAD_REQUEST,
                    "invalid_input",
                    "Image did not produce RGB data",
                );
            };
            vec![PreparedEmbedding::Image {
                image,
                text: text.join("\n"),
            }]
        }
    };
    let bytes = inputs.iter().try_fold(0u64, |bytes, input| {
        let len = match input {
            PreparedEmbedding::Text(text) => text.len(),
            PreparedEmbedding::Image { image, text } => {
                image.pixels().len().checked_add(text.len())?
            }
        };
        bytes.checked_add(u64::try_from(len).ok()?)
    });
    if bytes.is_none_or(|n| settings.media.limits.check_decoded_bytes(n).is_err()) {
        return error(
            StatusCode::BAD_REQUEST,
            "input_limit",
            "Embedding batch exceeds the media byte limit",
        );
    }
    let outputs = match state
        .host
        .onnx
        .embed(model, inputs, settings.clone(), payload.keep_alive)
        .await
    {
        Ok(outputs) => outputs,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "embedding_failed",
                format!("{e:#}"),
            )
        }
    };
    let mut data = Vec::new();
    let mut total_tokens = 0usize;
    let mut output_bytes = 0u64;
    for (index, output) in outputs.into_iter().enumerate() {
        let Some(tokens) = total_tokens.checked_add(output.tokens) else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "embedding_failed",
                "Token count overflow",
            );
        };
        total_tokens = tokens;
        let Some(bytes) = (output.vector.len() as u64)
            .checked_mul(4)
            .and_then(|n| output_bytes.checked_add(n))
        else {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "embedding_failed",
                "Output size overflow",
            );
        };
        output_bytes = bytes;
        if settings
            .media
            .limits
            .check_decoded_bytes(output_bytes)
            .is_err()
        {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "output_limit",
                "Embedding batch output exceeds byte limit",
            );
        }
        data.push(EmbeddingData {
            object: "embedding",
            embedding: output.vector,
            index,
        });
    }
    Json(EmbeddingResponse {
        object: "list",
        data,
        model: id,
        usage: EmbeddingUsage {
            prompt_tokens: total_tokens,
            total_tokens,
        },
    })
    .into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_requests_reject_empty_batches_audio_and_unsupported_encoding() -> anyhow::Result<()> {
        for body in [
            json!({"input":[]}),
            json!({"input":[""]}),
            json!({"input":"x","encoding_format":"base64"}),
            json!({"input":[{"type":"audio","source":"ignored"}]}),
        ] {
            assert!(validate(&serde_json::from_value(body)?).is_err());
        }
        assert!(validate(&serde_json::from_value(
            json!({"input":[{"type":"image","source":"data:ignored"}]})
        )?)
        .is_ok());
        Ok(())
    }
}
