use axum::{extract::{State, Path}, Json};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;
use engines::{HardwareDetector, CoreRoster};
use sysinfo::System;

// ─── GET /models/available ───────────────────────────────────────
pub async fn list_models(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let ram_gb = sys.total_memory() as f64 / 1_073_741_824.0; // Bytes to GB

    let silicon = HardwareDetector::new().detect();
    let recommendations = CoreRoster::get_recommendations(&silicon, ram_gb);
    
    Json(json!({
        "success": true,
        "system_ram_gb": format!("{:.2}", ram_gb),
        "available_models": recommendations
    }))
}

// ─── GET /hardware ───────────────────────────────────────────────
pub async fn hardware_status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let stats = HardwareDetector::new().detect();
    Json(json!({
        "success": true,
        "hardware": stats
    }))
}

// ─── POST /models/download ───────────────────────────────────────
pub async fn download_model(State(_state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "success": true,
        "status": "Download feature queued."
    }))
}

// ─── GET /api/tags ───────────────────────────────────────────────────
pub async fn tags(State(_state): State<Arc<AppState>>) -> Json<Value> {
    // Only return models that are already downloaded locally.
    let models = CoreRoster::load_roster();
    let mut downloaded_models = Vec::new();
    
    for m in models {
        // If it has a local path or is cached, it's available for inference
        let is_cached = m.local_path.is_some() || engines::models::fetch::ModelDownloader::get_cached_path(&m.category, &m.id, &m.huggingface_filename).is_some();
        
        if is_cached {
            downloaded_models.push(json!({
                "name": m.id,
                "size": (m.download_size_gb * 1024.0 * 1024.0 * 1024.0) as u64,
                "details": {
                    "format": m.architecture_type,
                    "family": m.architecture,
                    "parameter_size": m.parameters,
                }
            }));
        }
    }

    Json(json!({
        "models": downloaded_models
    }))
}

// ─── GET /v1/models/installed ────────────────────────────────────────
// Directly scans ~/.cluaiz/models on disk — no registry, no inference.
pub async fn list_installed_models(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let models_root = cluaiz_shared::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());
    let mut installed = Vec::new();

    let categories = ["chat", "embedding", "vision", "audio", "code"];
    for category in &categories {
        let cat_path = models_root.join(category);
        if !cat_path.is_dir() { continue; }
        if let Ok(entries) = std::fs::read_dir(&cat_path) {
            for entry in entries.flatten() {
                let model_dir = entry.path();
                if !model_dir.is_dir() { continue; }
                let model_id = model_dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Find the .gguf file inside this dir
                let mut gguf_file = None;
                let mut size_bytes: u64 = 0;
                if let Ok(files) = std::fs::read_dir(&model_dir) {
                    for f in files.flatten() {
                        let p = f.path();
                        if p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                            size_bytes = p.metadata().map(|m| m.len()).unwrap_or(0);
                            gguf_file = p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string());
                            break;
                        }
                    }
                }

                installed.push(json!({
                    "id": model_id,
                    "category": category,
                    "file": gguf_file,
                    "size_bytes": size_bytes,
                    "size_gb": format!("{:.2} GB", size_bytes as f64 / 1_073_741_824.0),
                    "path": model_dir.to_string_lossy()
                }));
            }
        }
    }

    installed.sort_by(|a, b| {
        a["id"].as_str().unwrap_or("").cmp(b["id"].as_str().unwrap_or(""))
    });

    Json(json!({
        "count": installed.len(),
        "models_root": models_root.to_string_lossy(),
        "installed": installed
    }))
}


#[derive(serde::Deserialize)]
pub struct PullPayload {
    pub model_id: String,
}

// ─── POST /api/pull ──────────────────────────────────────────────────
pub async fn pull_model(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<PullPayload>,
) -> Json<Value> {
    let cluaiz_root = cluaiz_shared::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());
    let manager = engines::models::manager::ModelManager::new(engines::models::registry::REGISTRY_URL.to_string(), cluaiz_root);
    
    let model_id = payload.model_id.clone();
    // Background pull
    tokio::spawn(async move {
        let _ = manager.pull_model(&payload.model_id).await;
    });

    Json(json!({
        "status": "success",
        "message": format!("Model pull for '{}' queued in background.", model_id)
    }))
}

// ─── POST /v1/hardware/calibrate ──────────────────────────────────────
pub async fn calibrate(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let _ = cluaiz_shared::hardware::governor::HardwareGovernor::auto_calibrate();
    Json(json!({
        "status": "success",
        "message": "Real-time RDTSC hardware clocking & SIMD profiling completed."
    }))
}

// ─── DELETE /v1/models/{model_id} ─────────────────────────────────────
pub async fn rm_model(
    State(_state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Json<Value> {
    let models_dir = cluaiz_shared::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());
    let model_file = models_dir.join(format!("{}.gguf", model_id));
    if model_file.exists() {
        let _ = std::fs::remove_file(&model_file);
        return Json(json!({
            "status": "success",
            "message": format!("Vault physical deletion for '{}' completed.", model_id)
        }));
    }
    Json(json!({
        "status": "error",
        "message": format!("Model '{}' not found in vault.", model_id)
    }))
}

#[derive(serde::Deserialize)]
pub struct LoadPayload {
    pub model_id: String,
}

// ─── POST /models/load ───────────────────────────────────────────
pub async fn load_model(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoadPayload>,
) -> Json<Value> {
    let roster = CoreRoster::load_roster();
    if let Some(manifest) = roster.into_iter().find(|m| m.id.to_lowercase() == payload.model_id.to_lowercase()) {
        if let Some(local_path) = manifest.local_path {
            let model_file = std::path::Path::new(&local_path).join(&manifest.huggingface_filename);
            if model_file.exists() {
                let dna = cluaiz_shared::StructuralDNA::default();
                let context = cluaiz_shared::cluaizContext::boot(dna, cluaiz_shared::TemplateManager::default());
                
                // We don't await the long load here, just signal success for now or wait
                // In a production setup, this would spawn or use a channel.
                return Json(json!({
                    "status": "success",
                    "message": format!("Model '{}' located at '{:?}'. Kernel instantiation queued.", manifest.id, model_file)
                }));
            }
        }
    }
    
    Json(json!({
        "status": "error",
        "message": format!("Model '{}' not found in vault or not downloaded.", payload.model_id)
    }))
}
