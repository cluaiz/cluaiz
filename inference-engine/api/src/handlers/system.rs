use axum::{
    extract::State,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::state::AppState;



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
    let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
    Json(json!({
        "engine": "cluaiz",
        "full_name": "cluaiz Inference Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "pillars": {
            "api": format!("Gateway — HTTP server on port {} (this!)", perms.api_port),
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

// ─── Cancel Generation ────────────────────────────────────────────────
pub async fn cancel_generation() -> Json<Value> {
    cluaiz_shared::GLOBAL_CANCEL_SIGNAL.store(true, std::sync::atomic::Ordering::SeqCst);
    Json(json!({
        "status": "success",
        "message": "Global cancel signal triggered. Active inference stopped."
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

// ─── GET /v1/system/gguf_config ───────────────────────────────────────
pub async fn get_gguf_config() -> Json<Value> {
    use cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders;
    let config = GgufMetadataHeaders::load();
    Json(serde_json::to_value(config).unwrap_or(json!({})))
}

// ─── POST /v1/system/gguf_config ──────────────────────────────────────
pub async fn update_gguf_config(Json(payload): Json<cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders>) -> Json<Value> {
    match payload.save() {
        Ok(_) => Json(json!({"status": "success"})),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()}))
    }
}

// ─── GET /v1/system/onnx_config ───────────────────────────────────────
pub async fn get_onnx_config() -> Json<Value> {
    use cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders;
    let config = OnnxMetadataHeaders::load();
    Json(serde_json::to_value(config).unwrap_or(json!({})))
}

// ─── POST /v1/system/onnx_config ──────────────────────────────────────
pub async fn update_onnx_config(Json(payload): Json<cluaiz_shared::hardware::schema::onnx_metadata::OnnxMetadataHeaders>) -> Json<Value> {
    match payload.save() {
        Ok(_) => Json(json!({"status": "success"})),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()}))
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
