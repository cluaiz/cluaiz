//! 🌐 Cluaiz REST & Streaming API Server (`api/`)
//! Single unified HTTP/SSE gateway.

pub mod auth;
pub mod handlers;
pub mod routes;
pub mod state;
pub mod url_resolver;
pub mod utils;

use axum::Router;
use std::sync::Arc;
pub use state::AppState;

pub fn create_router(state: AppState) -> Router {
    let shared_state = Arc::new(state);
    routes::build_api_router(shared_state)
}

/// Boots and runs the unified HTTP/SSE daemon server.
pub async fn run_daemon() -> anyhow::Result<()> {
    let host = Arc::new(cluaiz_runtime::EngineHost::new());
    let perms = cluaiz_security::PermissionSchema::load();
    let state = AppState::new(host.clone());
    let app = create_router(state);

    // Optional background async warmup if lazy_load_model is disabled (never blocks daemon port binding)
    if !perms.lazy_load_model {
        let host_clone = host.clone();
        let perms_clone = perms.clone();
        tokio::spawn(async move {
            if let Some(model_path) = crate::utils::slots::resolve_model_path(&perms_clone, "chat_slot") {
                let model_id = perms_clone.active_slots.get("chat_slot").and_then(|s| s.model_id.as_deref()).unwrap_or("default");
                match host_clone.ensure_loaded(model_id, &model_path, &cluaiz_core::Settings::current()).await {
                    Ok(_) => tracing::info!("🚀 [EngineHost] Warmup load done: {:?}", model_path),
                    Err(e) => tracing::warn!("⚠️ [EngineHost] Warmup skipped: {}", e),
                }
            }
        });
    }
    let port: u16 = std::env::var("CLUAIZ_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(perms.api_port);
    let host_ip = std::env::var("CLUAIZ_HOST")
        .unwrap_or_else(|_| perms.api_host.clone());
    let addr = format!("{}:{}", host_ip, port);

    tracing::info!("🌐 Cluaiz API Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
