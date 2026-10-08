//! 🌐 Cluaiz REST & Streaming API Server (`api/`)
//! Single unified HTTP/SSE gateway.

pub mod handlers;
pub mod utils;

use axum::{routing::get, Router};
use std::sync::Arc;
use cluaiz_runtime::EngineHost;

#[derive(Clone)]
pub struct AppState {
    pub host: Arc<EngineHost>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/v1/models", get(handlers::models::list_models))
        .with_state(state)
}
