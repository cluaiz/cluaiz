//! `GET /health`: the liveness probe. Everything else about the system is CEP `system.*`.

use axum::response::Json;
use serde_json::{json, Value};

// ─── Health Check ────────────────────────────────────────────────────
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "engine": "Cluaiz Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "message": "Cluaiz Engine is alive and operational."
    }))
}
