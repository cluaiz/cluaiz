use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_hw::HardwareProber;

// ─── POST /v1/system/profile ──────────────────────────────────────────
pub async fn configure_profile(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let hw = HardwareProber::current();
    Json(json!({
        "status": "success",
        "message": "Hardware environment profile probed and verified.",
        "hardware": hw
    }))
}
