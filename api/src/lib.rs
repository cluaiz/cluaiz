//! 🌐 Cluaiz REST & Streaming API Server (`api/`)
//! Single unified HTTP/SSE gateway.

pub mod auth;
pub mod handlers;
pub mod native;
pub mod routes;
pub mod state;
pub mod url_resolver;
pub mod utils;

use axum::Router;
pub use state::AppState;
use std::sync::Arc;

pub fn create_router(state: AppState) -> Router {
    let shared_state = Arc::new(state);
    routes::build_api_router(shared_state)
}

/// Boots and runs the unified HTTP/SSE daemon server.
pub async fn run_daemon() -> anyhow::Result<()> {
    run_daemon_with_native_endpoint(None).await
}

/// The explicit native mode serves the same router without opening a TCP listener.
pub async fn run_daemon_with_native_endpoint(endpoint: Option<String>) -> anyhow::Result<()> {
    let native = endpoint.as_deref().map(native::NativeListener::bind).transpose()?;
    // Register model folders that are on disk but not in model_registry.json (as the server did before)
    cluaiz_models::registry::InstalledStateRegistry::sync_from_disk(
        &cluaiz_models::registry::ModelVault::root_dir(),
    );
    let host = Arc::new(cluaiz_runtime::EngineHost::new());
    let perms = cluaiz_security::PermissionSchema::load();
    let state = AppState::new(host.clone());
    let session_token = state.session_token.clone();
    let app = create_router(state);

    // Optional background async warmup if lazy_load_model is disabled (never blocks daemon port binding)
    let warmup = if !perms.lazy_load_model {
        let host_clone = host.clone();
        let perms_clone = perms.clone();
        Some(tokio::spawn(async move {
            if let Some(model_path) =
                crate::utils::slots::resolve_model_path(&perms_clone, "chat_slot")
            {
                let model_id = perms_clone
                    .active_slots
                    .get("chat_slot")
                    .and_then(|s| s.model_id.as_deref())
                    .unwrap_or("default");
                match host_clone
                    .ensure_loaded(model_id, &model_path, &cluaiz_core::Settings::current())
                    .await
                {
                    Ok(_) => tracing::info!("🚀 [EngineHost] Warmup load done: {:?}", model_path),
                    Err(e) => tracing::warn!("⚠️ [EngineHost] Warmup skipped: {}", e),
                }
            }
        }))
    } else { None };
    let result: anyhow::Result<()> = async {
        if let Some(listener) = native {
            tracing::info!("Cluaiz API using explicit local native endpoint");
            listener.serve(app, session_token, shutdown_signal()).await
        } else {
            let port = api_port(&perms);
            let host_ip = std::env::var("CLUAIZ_HOST").unwrap_or_else(|_| perms.api_host.clone());
            let addr = format!("{}:{}", host_ip, port);
            tracing::info!("🌐 Cluaiz API Server running on http://{}", addr);
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
            Ok(())
        }
    }.await;
    if let Some(task) = warmup {
        task.abort();
        let _ = task.await;
    }
    tokio::task::spawn_blocking(cluaiz_media::documents::release_native_providers).await??;
    result?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::warn!("Cannot listen for API shutdown: {error}");
        std::future::pending::<()>().await;
    }
}

/// Port the API server listens on: `CLUAIZ_PORT`, else permission.json `api_port`.
pub fn api_port(perms: &cluaiz_security::PermissionSchema) -> u16 {
    std::env::var("CLUAIZ_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(perms.api_port)
}
