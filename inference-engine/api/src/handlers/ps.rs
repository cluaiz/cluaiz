use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_shared::hardware::governor::HardwareGovernor;

// ─── GET /v1/system/ps ────────────────────────────────────────────────
pub async fn get_processes(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let registry = HardwareGovernor::load_process_registry();
    
    let roster = engines::CoreRoster::load_roster();
    let mut processes = Vec::new();
    for (pid_str, info) in registry {
        let original_ctx = roster.iter()
            .find(|m| m.id == info.model_id || m.huggingface_filename == info.model_id || m.id.contains(&info.model_id))
            .map(|m| m.context_window.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        processes.push(json!({
            "pid": pid_str,
            "model_id": info.model_id,
            "vram_gb": info.vram_gb,
            "context_size": info.context_size,
            "original_context": original_ctx,
            "engine": info.engine
        }));
    }

    Json(json!({
        "status": "success",
        "active_processes": processes
    }))
}
