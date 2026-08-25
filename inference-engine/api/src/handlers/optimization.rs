use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_shared::hardware::governor::HardwareGovernor;
use cluaiz_shared::hardware::schema::optimization::OptimizationControl;

// ─── GET /v1/optimization/status ──────────────────────────────────────────
pub async fn status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match HardwareGovernor::load_optimization_settings() {
        Ok(settings) => Json(json!({
            "status": "success",
            "optimization": settings
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to load optimization settings: {}", e)
        })),
    }
}

// ─── POST /v1/optimization/update ─────────────────────────────────────────
pub async fn update(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<OptimizationControl>,
) -> Json<Value> {
    match HardwareGovernor::save_optimization_settings(&payload) {
        Ok(_) => Json(json!({
            "status": "success",
            "message": "Optimization settings saved successfully."
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to save optimization settings: {}", e)
        })),
    }
}
