use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_core::settings::{OptimizationControl, Settings};

// ─── GET /v1/optimization/status ──────────────────────────────────────────
pub async fn status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let settings = Settings::current();
    Json(json!({
        "status": "success",
        "optimization": settings.optimization
    }))
}

// ─── POST /v1/optimization/update ─────────────────────────────────────────
pub async fn update(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<OptimizationControl>,
) -> Json<Value> {
    let mut current = Settings::current();
    current.optimization = payload;
    match Settings::update(current) {
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
