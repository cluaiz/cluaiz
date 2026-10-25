//! 💻 System, Telemetry & Hardware Info Handlers (`api/src/handlers/system.rs`)
//! Modular implementation using cluaiz-hw HardwareProber, cluaiz-plan Planner, and cluaiz-core Settings.

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

// ─── POST /v1/system/cmd (Secure Local Terminal Execution) ───────────
#[derive(serde::Deserialize)]
pub struct CmdPayload {
    pub command: String,
}

pub async fn execute_cmd(Json(payload): Json<CmdPayload>) -> Json<Value> {
    let cmd_trimmed = payload.command.trim();
    if cmd_trimmed.is_empty() {
        return Json(json!({ "status": "error", "output": "Command payload cannot be empty." }));
    }
    if cmd_trimmed.len() > 8192 {
        return Json(
            json!({ "status": "error", "output": "Command payload exceeds maximum permitted length of 8192 characters." }),
        );
    }

    // Execute local OS command asynchronously via Tokio process (non-blocking)
    #[cfg(target_os = "windows")]
    let output = tokio::process::Command::new("cmd")
        .args(["/C", cmd_trimmed])
        .output()
        .await;

    #[cfg(not(target_os = "windows"))]
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd_trimmed)
        .output()
        .await;

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
            Json(json!({ "status": "success", "output": final_out }))
        }
        Err(e) => Json(
            json!({ "status": "error", "output": format!("Failed to execute process: {}", e) }),
        ),
    }
}
