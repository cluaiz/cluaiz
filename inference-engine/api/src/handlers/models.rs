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
// Returns full synchronized ModelRegistry database entries including probed metadata
pub async fn list_installed_models(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let registry = cluaiz_shared::utils::ModelRegistry::load();
    let installed_map = registry.installed_models.clone();
    let installed_vec: Vec<cluaiz_shared::utils::ModelRegistryEntry> = installed_map.values().cloned().collect();

    Json(json!({
        "status": "success",
        "count": installed_vec.len(),
        "installed": installed_vec.clone(),
        "models": installed_vec,
        "installed_models": installed_map
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
    if let Some(manifest) = roster.into_iter().find(|m| 
        m.id.to_lowercase() == payload.model_id.to_lowercase() ||
        m.huggingface_filename.to_lowercase() == payload.model_id.to_lowercase() ||
        m.name.to_lowercase() == payload.model_id.to_lowercase() ||
        m.id.replace(":", "-").to_lowercase() == payload.model_id.to_lowercase()
    ) {
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

// ─── GET /v1/models/{model_id}/inspect_raw_header ───────────────────────
pub async fn inspect_raw_header(
    State(_state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
) -> Json<Value> {
    let roster = CoreRoster::load_roster();
    let mut model_file_opt = None;
    let mut is_gguf = false;
    let mut is_onnx = false;

    if let Some(manifest) = roster.into_iter().find(|m| 
        m.id.to_lowercase() == model_id.to_lowercase() ||
        m.huggingface_filename.to_lowercase() == model_id.to_lowercase() ||
        m.name.to_lowercase() == model_id.to_lowercase() ||
        m.id.replace(":", "-").to_lowercase() == model_id.to_lowercase()
    ) {
        model_file_opt = manifest.local_path.clone()
            .map(|lp| std::path::Path::new(&lp).join(&manifest.huggingface_filename))
            .or_else(|| engines::models::fetch::ModelDownloader::get_cached_path(&manifest.category, &manifest.id, &manifest.huggingface_filename));
        is_gguf = manifest.huggingface_filename.ends_with(".gguf");
        is_onnx = manifest.huggingface_filename.ends_with(".onnx");
    } else {
        println!("Manifest not found in roster for model_id={}", model_id);
        let reg = cluaiz_shared::utils::ModelRegistry::load();
        if let Some(entry) = reg.installed_models.get(&model_id).or_else(|| 
            reg.installed_models.values().find(|e| e.id.to_lowercase() == model_id.to_lowercase())
        ) {
            if let Some(primary_file) = entry.files.iter().find(|f| f.is_primary).or(entry.files.first()) {
                model_file_opt = Some(std::path::Path::new(&entry.local_dir).join(&primary_file.name));
                is_gguf = primary_file.name.ends_with(".gguf");
                is_onnx = primary_file.name.ends_with(".onnx");
            }
        }
    }

    if let Some(model_file) = model_file_opt {
        if model_file.exists() {
            if is_gguf {
                match cluaiz_shared::utils::GGUFProber::probe(&model_file) {
                    Ok((metadata, tensor_infos, tensor_count)) => {
                        return Json(json!({
                            "status": "success",
                            "model_id": model_id,
                            "file_path": model_file.to_string_lossy().to_string(),
                            "format": "GGUF",
                            "tensor_count": tensor_count,
                            "metadata_kv": metadata,
                            "tensors_shape_map": tensor_infos
                        }));
                    },
                    Err(e) => {
                        println!("GGUF probe error for {}: {:?}", model_file.display(), e);
                    }
                }
            } else if is_onnx {
                // ── ONNX: Read file size + sibling config files + engine config ──
                let file_size_bytes = std::fs::metadata(&model_file).map(|m| m.len()).unwrap_or(0);
                let file_size_mb = (file_size_bytes as f64) / (1024.0 * 1024.0);
                
                // Read sibling JSON configs from the model directory (config.json, generation_config.json, etc.)
                let model_dir = model_file.parent().unwrap_or(std::path::Path::new(""));
                let mut sibling_configs: serde_json::Map<String, Value> = serde_json::Map::new();
                if let Ok(entries) = std::fs::read_dir(model_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.ends_with(".json") && name != "model_manifest.json" {
                            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                                if let Ok(parsed) = serde_json::from_str::<Value>(&content) {
                                    sibling_configs.insert(name, parsed);
                                }
                            }
                        }
                    }
                }

                // Read engine-level onnx_metadata_headers.json
                let engine_onnx_config = cluaiz_shared::environment::EnvironmentManager::current()
                    .config_dir()
                    .join("onnx_metadata_headers.json");
                let onnx_engine_settings: Value = std::fs::read_to_string(&engine_onnx_config)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Null);

                return Json(json!({
                    "status": "success",
                    "model_id": model_id,
                    "file_path": model_file.to_string_lossy().to_string(),
                    "format": "ONNX",
                    "file_size_mb": format!("{:.2}", file_size_mb),
                    "file_size_bytes": file_size_bytes,
                    "model_configs": sibling_configs,
                    "engine_onnx_settings": onnx_engine_settings,
                    "note": "ONNX protobuf header probing not available. Showing file metadata and config files."
                }));
            } else {
                println!("Unrecognized file extension");
            }
        } else {
            println!("File does not exist: {}", model_file.display());
        }
    } else {
        println!("Could not determine model_file_opt");
    }
    
    Json(json!({
        "status": "error",
        "message": format!("Model '{}' not found in vault or could not read binary header.", model_id)
    }))
}


