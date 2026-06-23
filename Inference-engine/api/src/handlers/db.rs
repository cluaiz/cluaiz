use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use engines::memory::tensor_transducer::TensorTransducer;

#[derive(Deserialize)]
pub struct CdqlRequest {
    pub query: String,
}

#[derive(Serialize)]
pub struct CdqlResponse {
    pub result: String,
}

/// ─── POST /v1/db/execute ───────────────────────────────────────────────
/// Executes a raw CDQL query on the database.
pub async fn execute_cdql(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<CdqlRequest>,
) -> impl IntoResponse {
    let query = payload.query;
    
    // Pass the raw CDQL query directly to the FFI bridge
    match TensorTransducer::execute_raw_cdql(&query, None) {
        Ok(result) => Json(serde_json::json!({ "result": result })).into_response(),
        Err(err) => Json(serde_json::json!({ "error": err })).into_response(),
    }
}
