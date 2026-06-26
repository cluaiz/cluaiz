use axum::{
    extract::State,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

// ─── Root ────────────────────────────────────────────────────────────
pub async fn root() -> Json<Value> {
    Json(json!({
        "engine": "Cluaize Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "gateway": "http://localhost:8000",
        "endpoints": {
            "GET  /":                     "This welcome message",
            "GET  /api/tags":             "List external compatible models",
            "POST /api/pull":             "Pull external compatible model",
            "GET  /models/available":     "List legacy available models",
            "GET  /hardware":             "Detect system RAM/CPU to suggest models",
            "POST /models/download":      "Download .gguf from Hugging Face",
            "POST /models/load":          "Load a downloaded .gguf file",
            "DELETE /v1/models/:model_id":"Remove a model from vault",
            "GET  /health":               "Engine health check",
            "GET  /history/:session":     "Chat history for a session",
            "GET  /info":                 "System information & pillars",
            "GET  /sessions":             "List all chat sessions",
            "GET  /status/sidecars":      "Database sidecar status",
            "POST /chat":                 "Send message → get AI response",
            "POST /engine/skip_think":    "Skip thinking during generation",
            "POST /v1/db/execute":        "FFI Database Query",
            "POST /v1/system/brain":      "Toggle FFI Brain modes",
            "GET  /v1/system/ps":         "Get running processes",
            "GET  /v1/system/control":    "Get system control status",
            "GET  /v1/system/permission": "Get system permissions",
            "POST /v1/system/permission": "Update system permissions",
            "POST /v1/system/profile":    "Configure hardware profile",
            "GET  /v1/skills/list":       "List all available WASM skills",
            "POST /v1/skills/install":    "Install a new WASM skill",
            "GET  /v1/skills/cache":      "List skill cache",
            "DELETE /v1/skills/cache":    "Clear skill cache",
            "GET  /v1/booster/status":    "Get hardware booster status",
            "POST /v1/booster/update":    "Update hardware booster configuration",
            "POST /v1/ingest/file":       "Ingest file into vector database",
            "POST /v1/benchmark/run":     "Run hardware benchmark suite",
            "POST /v1/hardware/calibrate":"Calibrate hardware settings"
        },
        "philosophy": "Nothing Need. Just Cluaize."
    }))
}

// ─── Health Check ────────────────────────────────────────────────────
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "engine": "Cluaize Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "message": "🚀 Cluaize is alive! All systems operational."
    }))
}

// ─── System Info ─────────────────────────────────────────────────────
pub async fn system_info() -> Json<Value> {
    Json(json!({
        "engine": "Cluaize",
        "full_name": "Cluaize Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "pillars": {
            "api": "Gateway — HTTP server on port 8000 (this!)",
            "kernel": "Brain — Decision-making & orchestration",
            "storage": "Sidecars — 5 Official DB engines (Mongo, Neo4j, ClickHouse, Qdrant, MinIO)",
            "engines": "Muscles — C++ model inference via llama.cpp FFI"
        },
        "philosophy": "Nothing Need. Just Cluaize.",
        "banned": ["Python", "Docker", "npm", "pip"]
    }))
}

// ─── Skip Thinking ───────────────────────────────────────────────────
pub async fn skip_think() -> Json<Value> {
    cluaize_shared::GLOBAL_SKIP_THINKING_SIGNAL.store(true, std::sync::atomic::Ordering::SeqCst);
    Json(json!({
        "status": "success",
        "message": "Brain skip signal injected. Neural graph will pivot."
    }))
}

// ─── GET /v1/system/control ───────────────────────────────────────────
pub async fn get_system_control(State(_state): State<Arc<AppState>>) -> Json<Value> {
    use cluaize_shared::hardware::governor::HardwareGovernor;
    if let Ok(control) = HardwareGovernor::load_system_control() {
        Json(json!({
            "status": "success",
            "control": control
        }))
    } else {
        Json(json!({
            "status": "error",
            "message": "Failed to load system control config"
        }))
    }
}

// ─── POST /v1/system/brain ────────────────────────────────────────────
#[derive(serde::Deserialize)]
pub struct BrainControlPayload {
    pub state: bool,
}

pub async fn toggle_brain(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<BrainControlPayload>,
) -> Json<Value> {
    use cluaize_shared::hardware::governor::HardwareGovernor;
    use cluaize_shared::hardware::system_control::HardwareOrchestrator;
    
    if let Ok(mut control) = HardwareGovernor::load_system_control() {
        control.brain.cluaizd_connect_ffi = if payload.state { "on".to_string() } else { "off".to_string() };
        if let Err(e) = HardwareOrchestrator::persist_sovereign_state(&control) {
            return Json(json!({
                "status": "error",
                "message": format!("Failed to save system control: {}", e)
            }));
        } else {
            return Json(json!({
                "status": "success",
                "message": format!("Pure Brain Mode toggled to: {}", payload.state)
            }));
        }
    }
    
    Json(json!({
        "status": "error",
        "message": "Failed to load system control config"
    }))
}

// ─── JIT Mid-Layer Injection API ──────────────────────────────────────────
pub async fn jit_inject() -> Json<Value> {
    // This is a placeholder for the explicit JIT Injection API.
    // Natively, injection happens via CEL tag interception in ffi_bridge.rs.
    // This API allows external orchestration tools to manually trigger an injection.
    tracing::info!("💉 [JIT Injection] Received explicit mid-layer injection request via HTTP.");
    Json(json!({
        "status": "success",
        "message": "JIT injection payload queued for next inference step.",
        "layer": "auto"
    }))
}
