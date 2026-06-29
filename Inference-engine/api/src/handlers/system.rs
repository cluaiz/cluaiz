use axum::{
    extract::State,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;

// ─── Root API Explorer (HTML UI) ───────────────────────────────────────
pub async fn root() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../../../assets/developer_hub/index.html"))
}

pub async fn style_css() -> impl axum::response::IntoResponse {
    let css = include_bytes!("../../../../assets/developer_hub/style.css");
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        css.as_slice()
    )
}

pub async fn app_js() -> impl axum::response::IntoResponse {
    let js = include_bytes!("../../../../assets/developer_hub/app.js");
    (
        [(axum::http::header::CONTENT_TYPE, "application/javascript")],
        js.as_slice()
    )
}

// ─── Favicon (Logo) ─────────────────────────────────────────────────────
pub async fn favicon() -> impl axum::response::IntoResponse {
    let icon = include_bytes!("../../../../assets/logo.ico");
    (
        [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
        icon.as_slice()
    )
}

// ─── API Data JSON ──────────────────────────────────────────────────────
pub async fn api_docs_json() -> Json<Value> {
    let system_json = include_str!("../../../../assets/developer_hub/data/system.json");
    let inference_json = include_str!("../../../../assets/developer_hub/data/inference.json");
    let models_json = include_str!("../../../../assets/developer_hub/data/models.json");
    let skills_json = include_str!("../../../../assets/developer_hub/data/skills.json");
    let plugins_json = include_str!("../../../../assets/developer_hub/data/plugins.json");
    let extensions_json = include_str!("../../../../assets/developer_hub/data/extensions.json");
    let mcp_json = include_str!("../../../../assets/developer_hub/data/mcp.json");
    let tuning_json = include_str!("../../../../assets/developer_hub/data/tuning.json");
    
    let system: Value = serde_json::from_str(system_json).unwrap_or(json!({}));
    let inference: Value = serde_json::from_str(inference_json).unwrap_or(json!({}));
    let models: Value = serde_json::from_str(models_json).unwrap_or(json!({}));
    let skills: Value = serde_json::from_str(skills_json).unwrap_or(json!({}));
    let plugins: Value = serde_json::from_str(plugins_json).unwrap_or(json!({}));
    let extensions: Value = serde_json::from_str(extensions_json).unwrap_or(json!({}));
    let mcp: Value = serde_json::from_str(mcp_json).unwrap_or(json!({}));
    let tuning: Value = serde_json::from_str(tuning_json).unwrap_or(json!({}));
    
    Json(json!([system, inference, models, skills, plugins, extensions, mcp, tuning]))
}

// ─── Health Check ────────────────────────────────────────────────────
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "engine": "cluaiz Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "message": "🚀 cluaiz is alive! All systems operational."
    }))
}

// ─── System Info ─────────────────────────────────────────────────────
pub async fn system_info() -> Json<Value> {
    Json(json!({
        "engine": "cluaiz",
        "full_name": "cluaiz Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "pillars": {
            "api": "Gateway — HTTP server on port 8000 (this!)",
            "kernel": "Brain — Decision-making & orchestration",
            "storage": "Sidecars — 5 Official DB engines (Mongo, Neo4j, ClickHouse, Qdrant, MinIO)",
            "engines": "Muscles — C++ model inference via llama.cpp FFI"
        },
        "philosophy": "Nothing Need. Just cluaiz.",
        "banned": ["Python", "Docker", "npm", "pip"]
    }))
}

// ─── Skip Thinking ───────────────────────────────────────────────────
pub async fn skip_think() -> Json<Value> {
    cluaiz_shared::GLOBAL_SKIP_THINKING_SIGNAL.store(true, std::sync::atomic::Ordering::SeqCst);
    Json(json!({
        "status": "success",
        "message": "Brain skip signal injected. Neural graph will pivot."
    }))
}

// ─── GET /v1/system/control ───────────────────────────────────────────
pub async fn get_system_control(State(_state): State<Arc<AppState>>) -> Json<Value> {
    use cluaiz_shared::hardware::governor::HardwareGovernor;
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




// ─── Execute Local Shell Command (Secure Web Terminal) ─────────────
#[derive(serde::Deserialize)]
pub struct CmdPayload {
    pub command: String,
}

pub async fn execute_cmd(
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<CmdPayload>,
) -> Json<Value> {
    // 1. Strict Security Check: Localhost ONLY.
    if !addr.ip().is_loopback() {
        tracing::error!("🚨 SECURITY ALERT: Remote attempt to execute command rejected from IP: {}", addr.ip());
        return Json(json!({
            "status": "error",
            "output": format!("Access Denied: 403 Forbidden. Execution strictly restricted to localhost (127.0.0.1). Request from {} blocked.", addr.ip())
        }));
    }

    // 2. Execute Command
    tracing::info!("Executing local command from DevHub UI: {}", payload.command);

    #[cfg(target_os = "windows")]
    let output = std::process::Command::new("cmd")
        .args(["/C", &payload.command])
        .output();

    #[cfg(not(target_os = "windows"))]
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&payload.command)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            
            let final_out = if !stderr.is_empty() && stdout.is_empty() {
                stderr
            } else if !stderr.is_empty() {
                format!("{}\n{}", stdout, stderr)
            } else {
                stdout
            };

            Json(json!({
                "status": "success",
                "output": final_out
            }))
        }
        Err(e) => {
            Json(json!({
                "status": "error",
                "output": format!("Failed to execute process: {}", e)
            }))
        }
    }
}
