use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_core::settings::Settings;

// ─── GET /v1/optimization/status ──────────────────────────────────────────
pub async fn status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let settings = Settings::current();
    Json(json!({
        "status": "success",
        "optimization": settings.optimization
    }))
}

// ─── POST /v1/optimization/update ─────────────────────────────────────────
// Clients send only the keys they change: merge them over the saved settings
// (replacing the whole struct reset every other setting to its default).
pub async fn update(
    State(_state): State<Arc<AppState>>,
    Json(patch): Json<Value>,
) -> (StatusCode, Json<Value>) {
    let mut current = Settings::current();
    current.optimization = match current.optimization.merged_with(&patch) {
        Ok(merged) => merged,
        Err(e) => {
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({
                "status": "error",
                "message": format!("Invalid optimization setting: {}", e)
            })));
        }
    };
    match Settings::update(current) {
        Ok(_) => (StatusCode::OK, Json(json!({
            "status": "success",
            "message": "Optimization settings saved successfully."
        }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "status": "error",
            "message": format!("Failed to save optimization settings: {}", e)
        }))),
    }
}
