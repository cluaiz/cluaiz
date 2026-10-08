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



// ─── GET /v1/system/info ──────────────────────────────────────────────
pub async fn get_system_info(State(_state): State<Arc<AppState>>) -> Json<Value> {
    use engine_core::hardware::governor::HardwareGovernor;
    if let Ok(info) = HardwareGovernor::load_system_info() {
        Json(json!({
            "status": "success",
            "info": info
        }))
    } else {
        Json(json!({
            "status": "error",
            "message": "Failed to load system info config"
        }))
    }
}

pub async fn get_system_control(state: State<Arc<AppState>>) -> Json<Value> {
    get_system_info(state).await
}

// ─── GET /v1/system/live (Live Real-Time Hardware State & Engine Footprint) ──
pub async fn get_system_live() -> Json<Value> {
    use sysinfo::{System, Pid};
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let used_ram_gb = sys.used_memory() as f64 / 1_073_741_824.0;
    let free_ram_gb = sys.available_memory() as f64 / 1_073_741_824.0;
    let cpu_utilization_pct = sys.global_cpu_usage();
    let per_core_usage: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();

    let current_pid = std::process::id();
    let pid = Pid::from_u32(current_pid);
    let (engine_rss_mb, engine_cpu_pct) = if let Some(proc) = sys.process(pid) {
        (proc.memory() as f64 / 1_048_576.0, proc.cpu_usage())
    } else {
        (0.0, 0.0)
    };

    // Live GPU metrics from pulse
    let pulse_state = engine_core::hardware::system_performance::get_pulse();
    let pulse = pulse_state.pulse.read().map(|p| p.clone()).unwrap_or_default();

    Json(json!({
        "status": "success",
        "timestamp": pulse.timestamp,
        "ram": {
            "total_gb": (total_ram_gb * 100.0).round() / 100.0,
            "used_gb": (used_ram_gb * 100.0).round() / 100.0,
            "free_gb": (free_ram_gb * 100.0).round() / 100.0,
            "utilization_pct": if total_ram_gb > 0.0 { ((used_ram_gb / total_ram_gb) * 100.0).round() } else { 0.0 },
            "engine_rss_mb": (engine_rss_mb * 100.0).round() / 100.0,
        },
        "cpu": {
            "utilization_pct": (cpu_utilization_pct * 10.0).round() / 10.0,
            "clock_ghz": pulse.cpu.clock_ghz,
            "temperature_c": pulse.cpu.temperature_c,
            "engine_cpu_pct": (engine_cpu_pct * 10.0).round() / 10.0,
            "per_core_pct": per_core_usage,
        },
        "vram": {
            "total_gb": (pulse.vram_total_gb * 100.0).round() / 100.0,
            "used_gb": (pulse.vram_used_gb * 100.0).round() / 100.0,
            "free_gb": (((pulse.vram_total_gb - pulse.vram_used_gb).max(0.0)) * 100.0).round() / 100.0,
            "pressure_pct": pulse.vram_pressure_pct,
        },
        "gpus": pulse.gpus,
        "storage": {
            "throughput_mbps": pulse.storage_throughput_mbps,
        },
        "network": {
            "throughput_mbps": pulse.network_throughput_mbps,
        },
        "engine_process": {
            "pid": current_pid,
            "memory_rss_mb": (engine_rss_mb * 100.0).round() / 100.0,
            "cpu_usage_pct": (engine_cpu_pct * 10.0).round() / 10.0,
        }
    }))
}

#[derive(serde::Deserialize, Default)]
pub struct LatencyComparePayload {
    pub model: Option<String>,
    pub weights_gb: Option<f64>,
    pub model_path: Option<String>,
    pub batch_tokens: Option<usize>,
    pub total_layers: Option<usize>,
    pub n_ctx: Option<usize>,
    pub vram_buffer_gb: Option<f64>,
    pub ram_buffer_gb: Option<f64>,
}

// ─── POST /v1/system/plan (Autonomous Model Execution Planning) ──────────────
pub async fn plan_execution(Json(payload): Json<LatencyComparePayload>) -> Json<Value> {
    use engine_core::hardware::resource_negotiator::{plan_execution_candidates, ExecutionPlanRequest};

    let req = ExecutionPlanRequest {
        model: payload.model,
        weights_gb: payload.weights_gb,
        model_path: payload.model_path.map(std::path::PathBuf::from),
        batch_tokens: payload.batch_tokens,
        total_layers: payload.total_layers,
        n_ctx: payload.n_ctx,
        vram_buffer_gb: payload.vram_buffer_gb,
        ram_buffer_gb: payload.ram_buffer_gb,
    };

    match plan_execution_candidates(&req) {
        Ok(decision) => Json(json!({
            "status": "success",
            "active_user_settings": decision.active_user_settings,
            "model_info": decision.model_info,
            "execution_plan": decision.execution_plan,
            "hardware_info": decision.hardware_info
        })),
        Err(err) => Json(json!({
            "status": "error",
            "error": err
        })),
    }
}


// ─── GET /v1/system/gguf_config ───────────────────────────────────────
pub async fn get_gguf_config() -> Json<Value> {
    use engine_core::hardware::schema::gguf_metadata::GgufMetadataHeaders;
    let config = GgufMetadataHeaders::load();
    Json(serde_json::to_value(config).unwrap_or(json!({})))
}

// ─── POST /v1/system/gguf_config ──────────────────────────────────────
pub async fn update_gguf_config(Json(payload): Json<engine_core::hardware::schema::gguf_metadata::GgufMetadataHeaders>) -> Json<Value> {
    match payload.save() {
        Ok(_) => Json(json!({"status": "success"})),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()}))
    }
}

// ─── GET /v1/system/onnx_config ───────────────────────────────────────
pub async fn get_onnx_config() -> Json<Value> {
    use engine_core::hardware::schema::onnx_metadata::OnnxMetadataHeaders;
    let config = OnnxMetadataHeaders::load();
    Json(serde_json::to_value(config).unwrap_or(json!({})))
}

// ─── POST /v1/system/onnx_config ──────────────────────────────────────
pub async fn update_onnx_config(Json(payload): Json<engine_core::hardware::schema::onnx_metadata::OnnxMetadataHeaders>) -> Json<Value> {
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
