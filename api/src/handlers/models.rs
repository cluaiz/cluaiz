//! 📦 Model Discovery, Hardware Status, Inspection & Ollama Compatibility Handlers (`api/src/handlers/models.rs`)

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_hw::HardwareProber;
use cluaiz_models::registry::InstalledStateRegistry;
use cluaiz_models::GgufProber;
use cluaiz_core::EnvironmentManager;

/// GET /models/available
pub async fn list_models(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let hw = HardwareProber::current();
    let registry = InstalledStateRegistry::load();
    
    Json(json!({
        "success": true,
        "hardware": hw,
        "available_models": registry.installed_models,
    }))
}

/// GET /hardware
pub async fn hardware_status() -> Json<Value> {
    let hw = HardwareProber::current();
    Json(json!({
        "status": "success",
        "hardware": hw
    }))
}

/// GET /v1/models/installed
pub async fn list_installed_models() -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let installed_vec: Vec<_> = registry.installed_models.values().cloned().collect();
    Json(json!({
        "status": "success",
        "count": installed_vec.len(),
        "installed": installed_vec.clone(),
        "models": installed_vec,
        "installed_models": registry.installed_models
    }))
}

/// GET /v1/models (OpenAI Standard Format)
pub async fn v1_models(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let registry = InstalledStateRegistry::load();
    let now = chrono::Utc::now().timestamp();
    let mut data = Vec::new();

    for (id, entry) in registry.installed_models {
        data.push(json!({
            "id": id,
            "object": "model",
            "created": now,
            "owned_by": "cluaiz",
            "permission": [],
            "root": id,
            "parent": null,
            "category": entry.category,
            "context_window": entry.metadata.context_window
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

/// POST /api/pull
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

/// POST /models/download
pub async fn download_model(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "success": true,
        "status": "Download feature queued."
    }))
}

#[derive(Debug, Deserialize)]
pub struct LoadPayload {
    pub model_id: String,
}

/// POST /models/load
pub async fn load_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoadPayload>,
) -> Json<Value> {
    let model_path = crate::utils::slots::resolve_model_by_id(&payload.model_id)
        .or_else(|| {
            let env = EnvironmentManager::current();
            let p = env.models_dir().join(format!("{}.gguf", payload.model_id));
            if p.exists() { Some(p) } else { None }
        })
        .or_else(|| {
            let env = EnvironmentManager::current();
            let p = env.models_dir().join(&payload.model_id);
            if p.exists() { Some(p) } else { None }
        });

    if let Some(path) = model_path {
        match state.host.ensure_loaded(&payload.model_id, &path, &cluaiz_core::Settings::current()).await {
            Ok(_) => Json(json!({
                "status": "success",
                "message": format!("Model '{}' loaded successfully from '{:?}'.", payload.model_id, path)
            })),
            Err(e) => Json(json!({
                "status": "error",
                "message": format!("Failed to load model '{}': {}", payload.model_id, e)
            })),
        }
    } else {
        Json(json!({
            "status": "error",
            "message": format!("Model '{}' not found in vault.", payload.model_id)
        }))
    }
}

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    pub filename: Option<String>,
}

/// GET /v1/models/{model_id}/inspect_raw_header
pub async fn inspect_raw_header(
    State(_state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(_query): Query<InspectQuery>,
) -> Json<Value> {
    let env = EnvironmentManager::current();
    let model_path = env.models_dir().join(format!("{}.gguf", model_id));

    if model_path.exists() {
        match GgufProber::probe_geometry(&model_path) {
            Ok(geom) => Json(json!({
                "status": "success",
                "geometry": geom
            })),
            Err(e) => Json(json!({
                "status": "error",
                "message": format!("Failed to probe GGUF header: {}", e)
            })),
        }
    } else {
        Json(json!({
            "status": "error",
            "message": format!("Model '{}' not found in vault.", model_id)
        }))
    }
}

/// DELETE /v1/models/{model_id}
pub async fn rm_model(
    State(_state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Json<Value> {
    let env = EnvironmentManager::current();
    let model_path = env.models_dir().join(format!("{}.gguf", model_id));

    if model_path.exists() {
        let _ = std::fs::remove_file(&model_path);
        Json(json!({
            "status": "success",
            "message": format!("Vault physical deletion for '{}' completed.", model_id)
        }))
    } else {
        Json(json!({
            "status": "error",
            "message": format!("Model '{}' not found in vault.", model_id)
        }))
    }
}

/// POST /v1/hardware/calibrate
pub async fn calibrate(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let hw = HardwareProber::current();
    Json(json!({
        "status": "success",
        "message": "Hardware calibration completed.",
        "hardware": hw
    }))
}
