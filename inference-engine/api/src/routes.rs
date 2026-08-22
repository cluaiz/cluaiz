use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;

use crate::state::AppState;
use crate::handlers::{chat, system, models, ingest, embeddings, audio};

pub fn build(state: Arc<AppState>) -> Router {
    // ── CORS — Restrict to localhost origins only (Desktop, Mobile apps on localhost) ──
    // allow_origin(Any) would allow any web page to call this local engine and exfiltrate data.
    let cors = CorsLayer::permissive();

    Router::new()
        .route("/health", get(system::health_check))
        .route("/info", get(system::system_info))
        .route("/engine/skip_think", post(system::skip_think))
        .route("/v1/system/cmd", post(system::execute_cmd))
        
        // ── External Compatible Streaming, Embeddings & Audio API ──
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/v1/embeddings", post(embeddings::generate_embeddings))
        .route("/v1/audio/speech", post(audio::speech))
        .route("/v1/audio/transcriptions", post(audio::transcriptions))
        .route("/v1/audio/execute", post(audio::execute_audio))
        
        // ── External Compatible Models API ──
        .route("/api/tags", get(models::tags))
        .route("/api/pull", post(models::pull_model))
        
        // ── Legacy Models API ──
        .route("/models/available", get(models::list_models))
        .route("/v1/models/installed", get(models::list_installed_models))
        .route("/hardware", get(models::hardware_status))
        .route("/models/download", post(models::download_model))
        .route("/models/load", post(models::load_model))
        .route("/v1/models/{model_id}/inspect_raw_header", get(models::inspect_raw_header))

        
        // ── Optimization & Hardware Tuning API ──
        .route("/v1/optimization/status", get(crate::handlers::booster::status))
        .route("/v1/optimization/update", post(crate::handlers::booster::update))


        // ── WASM Skills & Agents API ──
        .route("/v1/skills/list", get(crate::handlers::skills::list_skills))
        .route("/v1/skills/install", post(crate::handlers::skills::install_skill))
        .route("/v1/skills/remove", axum::routing::delete(crate::handlers::skills::remove_skill))
        .route("/v1/skills/cache", get(crate::handlers::skills::list_cache))
        .route("/v1/skills/cache", axum::routing::delete(crate::handlers::skills::clear_cache))

        // ── Plugins API ──
        .route("/v1/plugins/list", get(crate::handlers::plugins::list_plugins))
        .route("/v1/plugins/install", post(crate::handlers::plugins::install_plugin))
        .route("/v1/plugins/remove", axum::routing::delete(crate::handlers::plugins::remove_plugin))
        .route("/v1/plugins/cache", get(crate::handlers::plugins::list_cache))
        .route("/v1/plugins/cache", axum::routing::delete(crate::handlers::plugins::clear_cache))

        // ── Extensions API ──
        .route("/v1/extensions/list", get(crate::handlers::extensions::list_extensions))
        .route("/v1/extensions/install", post(crate::handlers::extensions::install_extension))
        .route("/v1/extensions/remove", axum::routing::delete(crate::handlers::extensions::remove_extension))
        .route("/v1/extensions/cache", get(crate::handlers::extensions::list_cache))
        .route("/v1/extensions/cache", axum::routing::delete(crate::handlers::extensions::clear_cache))

        // ── MCP API ──
        .route("/v1/mcp/list", get(crate::handlers::mcp::list_mcp))
        .route("/v1/mcp/install", post(crate::handlers::mcp::install_mcp))
        .route("/v1/mcp/remove", axum::routing::delete(crate::handlers::mcp::remove_mcp))
        .route("/v1/mcp/cache", get(crate::handlers::mcp::list_cache))
        .route("/v1/mcp/cache", axum::routing::delete(crate::handlers::mcp::clear_cache))

        // ── Components API (Used by Dashboard) ──
        .route("/api/components/list", get(crate::handlers::components::list_components))
        .route("/api/components/settings", get(crate::handlers::components::get_settings).post(crate::handlers::components::update_settings))
        .route("/api/components/file", get(crate::handlers::components::get_specific_file).post(crate::handlers::components::update_file))
        .route("/api/components/files", get(crate::handlers::components::get_files))
        .route("/api/components/cache", post(crate::handlers::components::clear_cache))

        // ── Dynamic Ecosystem Execution Route ──
        .route("/v1/execute/{component_name}/{function_name}", post(crate::handlers::cel_handler::execute_dynamic))

        // ── Vector Ingest API ──
        .route("/v1/ingest", post(crate::handlers::ingest::file_ingest))
        .route("/v1/ingest/file", post(crate::handlers::ingest::file_ingest))

        // ── Hardware Benchmark Suite ──
        .route("/v1/benchmark/run", post(crate::handlers::benchmark::run))

        // ── System Control API (Phase 2) ──
        .route("/v1/system/ps", get(crate::handlers::ps::get_processes))
        .route("/v1/system/control", get(crate::handlers::system::get_system_control))
        .route("/v1/system/permission", get(crate::handlers::permission::get_permission))
        .route("/v1/system/permission", post(crate::handlers::permission::update_permission))
        
        // ── Storage Control API ──
        .route("/v1/system/storage/temp_media", get(crate::handlers::storage::get_temp_media_status))
        .route("/v1/system/storage/temp_media/clean", post(crate::handlers::storage::clean_temp_media))
        .route("/v1/system/storage/settings", get(crate::handlers::storage::get_storage_settings).post(crate::handlers::storage::update_storage_settings))
        .route("/v1/system/gguf_config", get(crate::handlers::system::get_gguf_config))
        .route("/v1/system/gguf_config", post(crate::handlers::system::update_gguf_config))
        .route("/v1/system/onnx_config", get(crate::handlers::system::get_onnx_config))
        .route("/v1/system/onnx_config", post(crate::handlers::system::update_onnx_config))



        // ── Pure CEL Execution API (Phase 3 / Ecosystem) ──
        .route("/v1/cel/execute", post(crate::handlers::cel_handler::execute_cel_script))


        // ── Hardware Calibration (Phase 2) ──
        .route("/v1/hardware/calibrate", post(crate::handlers::models::calibrate))

        // ── Vault Management (Phase 2) ──
        .route("/v1/models/{model_id}", axum::routing::delete(crate::handlers::models::rm_model))

        // ── Developer Hub (Embedded UI) ──
        .merge(cluaiz_devhub::devhub_routes())
        .layer(axum::middleware::from_fn_with_state(state.clone(), crate::auth::auth_middleware))
        .layer(cors)
        .with_state(state)
}
