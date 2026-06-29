use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_shared::hardware::governor::HardwareGovernor;
use cluaiz_shared::hardware::schema::booster::BoosterControl;

// ─── GET /v1/booster/status ───────────────────────────────────────────
pub async fn status(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match HardwareGovernor::load_booster_settings() {
        Ok(settings) => Json(json!({
            "status": "success",
            "booster": settings
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to load booster settings: {}", e)
        })),
    }
}

// ─── POST /v1/booster/update ──────────────────────────────────────────
pub async fn update(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<BoosterControl>,
) -> Json<Value> {
    match HardwareGovernor::save_booster_settings(&payload) {
        Ok(_) => Json(json!({
            "status": "success",
            "message": "Booster settings successfully locked into Sovereign Engine."
        })),
        Err(e) => Json(json!({
            "status": "error",
            "message": format!("Failed to save booster settings: {}", e)
        })),
    }
}
