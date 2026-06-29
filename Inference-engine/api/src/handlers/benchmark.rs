use axum::{Json, extract::State};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;

// ─── POST /v1/benchmark/run ───────────────────────────────────────────
pub async fn run(State(_state): State<Arc<AppState>>) -> Json<Value> {
    engines::telemetry::health_check::cluaizHealthChecker::run_full_benchmark();
    Json(json!({"status": "success", "message": "Hardware isolated benchmark stream executed."}))
}
