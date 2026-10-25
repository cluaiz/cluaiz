//! Standard OpenAI/Ollama model routes. CEP controls live in `cep::methods::models`.

use crate::AppState;
use axum::{extract::State, Json};
use cluaiz_models::registry::InstalledStateRegistry;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// GET /v1/models (OpenAI Standard Format)
pub async fn v1_models(State(state): State<Arc<AppState>>) -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let now = chrono::Utc::now().timestamp();
    let mut data = Vec::new();

    for (id, entry) in registry.installed_models {
        let task_support =
            if entry.format_type == "onnx" {
                if let Some(package) = entry.metadata.capabilities.as_ref() {
                    let loaded = state.host.onnx.readiness(&entry).await.ok().flatten();
                    let formats = cluaiz_media::formats::attachment_formats();
                    let routes: Vec<_> =
                        formats
                            .iter()
                            .map(|&(input, format)| {
                                cluaiz_runtime::media_support::AttachmentRoute { format, input }
                            })
                            .collect();
                    Some(cluaiz_runtime::media_support::resolve_onnx_support(
                        package,
                        loaded.as_ref(),
                        &routes,
                    ))
                } else {
                    None
                }
            } else {
                None
            };
        data.push(json!({
            "id": id,
            "object": "model",
            "created": now,
            "owned_by": "cluaiz",
            "permission": [],
            "root": id,
            "parent": null,
            "category": entry.category,
            "context_window": entry.metadata.context_window,
            "package_capabilities": entry.metadata.capabilities,
            "task_support": task_support
        }));
    }

    Json(json!({
        "object": "list",
        "data": data
    }))
}

/// GET /api/tags (Ollama Compatibility)
pub async fn tags() -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let tags: Vec<Value> = registry
        .installed_models
        .values()
        .map(|entry| {
            let total_size = entry.files.iter().map(|f| f.size_bytes).sum::<u64>();
            json!({
                "name": entry.id,
                "model": entry.id,
                "modified_at": "2026-10-08T00:00:00Z",
                "size": total_size,
                "details": {
                    "format": entry.format_type,
                    "family": entry.metadata.architecture,
                    "parameter_size": entry.metadata.parameters
                }
            })
        })
        .collect();

    Json(json!({ "models": tags }))
}

#[derive(Debug, Deserialize)]
pub struct PullPayload {
    pub model_id: String,
}

/// POST /api/pull. Pending O6: this body still reports queued without downloading.
pub async fn pull_model(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<PullPayload>,
) -> Json<Value> {
    let model_id = payload.model_id.clone();
    Json(json!({
        "status": "success",
        "message": format!("Model pull for '{}' queued natively in background.", model_id)
    }))
}
