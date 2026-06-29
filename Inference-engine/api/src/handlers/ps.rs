use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_shared::hardware::governor::HardwareGovernor;

// ─── GET /v1/system/ps ────────────────────────────────────────────────
pub async fn get_processes(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let registry = HardwareGovernor::load_process_registry();
    
    let mut processes = Vec::new();
    for (pid_str, info) in registry {
        processes.push(json!({
            "pid": pid_str,
            "model_id": info.model_id,
            "vram_gb": info.vram_gb,
            "context_size": info.context_size,
            "engine": info.engine
        }));
    }

    Json(json!({
        "status": "success",
        "active_processes": processes
    }))
}
