//! 💻 System, Telemetry & Hardware Info Handlers (`api/src/handlers/system.rs`)
//! Modular implementation using cluaiz-hw HardwareProber, cluaiz-plan Planner, and cluaiz-core Settings.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_core::Settings;
use cluaiz_hw::HardwareProber;

// ─── Health Check ────────────────────────────────────────────────────
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "alive",
        "engine": "Cluaiz Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "message": "Cluaiz Engine is alive and operational."
    }))
}

// ─── System Info (Engine Identity & Architecture) ────────────────────
pub async fn system_info() -> Json<Value> {
    Json(json!({
        "engine": "Cluaiz Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "architecture": {
            "api": "Unified HTTP/SSE Router",
            "runtime": "Single Engine Host (Static rlib)",
            "planner": "HardwareProber & ExecutionPlan",
            "security": "Filesystem Jail & Approval Guard"
        }
    }))
}

// ─── GET /v1/system/info & /v1/system/control ────────────────────────
pub async fn get_system_info(State(_state): State<Arc<AppState>>) -> Json<Value> {
    let hw = HardwareProber::current();
    Json(json!({
        "status": "success",
        "info": hw
    }))
}

pub async fn get_system_control(state: State<Arc<AppState>>) -> Json<Value> {
    get_system_info(state).await
}

// ─── GET /v1/system/live (Live Memory, CPU & Load) ───────────────────
pub async fn get_system_live() -> Json<Value> {
    let hw = HardwareProber::current();
    let current_pid = std::process::id();
    let used_gb = (hw.memory.total_gb - hw.memory.available_gb).max(0.0);

    Json(json!({
        "status": "success",
        "process_id": current_pid,
        "memory": {
            "total_gb": hw.memory.total_gb,
            "available_gb": hw.memory.available_gb,
            "used_gb": used_gb,
        },
        "cpu": {
            "brand": hw.cpu.brand,
            "physical_cores": hw.cpu.physical_cores,
            "logical_threads": hw.cpu.logical_threads,
            "frequency_mhz": hw.cpu.frequency_mhz,
        },
        "gpus": hw.gpus
    }))
}

// ─── POST /v1/system/plan (Autonomous Model Execution Planning) ──────
#[derive(serde::Deserialize, serde::Serialize, Default)]
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

pub async fn plan_execution(Json(payload): Json<LatencyComparePayload>) -> Json<Value> {
    let model_id = payload.model.unwrap_or_else(|| "default-model".to_string());
    let hw = HardwareProber::with_live_memory();
    let mut settings = Settings::current();
    if let Some(vb) = payload.vram_buffer_gb {
        settings.optimization.custom_vram_buffer_gb = Some(vb);
    }
    if let Some(rb) = payload.ram_buffer_gb {
        settings.optimization.custom_ram_buffer_gb = Some(rb);
    }

    // Resolve real model geometry from provided path or InstalledStateRegistry
    let model_file_path = if let Some(ref path) = payload.model_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            Some(p)
        } else {
            None
        }
    } else {
        let reg = cluaiz_models::registry::InstalledStateRegistry::load();
        reg.installed_models.get(&model_id)
            .or_else(|| {
                let clean = model_id.trim_end_matches(".gguf");
                reg.installed_models.get(clean)
            })
            .and_then(|entry| {
                let dir = std::path::Path::new(&entry.local_dir);
                entry.files.iter()
                    .find(|f| f.is_primary && f.name.ends_with(".gguf"))
                    .map(|f| dir.join(&f.name))
                    .or_else(|| {
                        std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path())
                            .find(|p| p.extension().and_then(|ext| ext.to_str()) == Some("gguf"))
                    })
            })
    };

    let geometry = match model_file_path {
        Some(ref p) => match cluaiz_models::GgufProber::probe_geometry(p) {
            Ok(geom) => geom,
            Err(e) => {
                return Json(json!({
                    "status": "error",
                    "code": "gguf_probe_failed",
                    "message": format!("Failed to probe GGUF geometry at '{}': {}", p.display(), e)
                }));
            }
        },
        None => {
            return Json(json!({
                "status": "error",
                "code": "model_not_found",
                "message": format!("Model '{}' not found in registry and no valid model_path was provided. Zero dummy estimates permitted.", model_id)
            }));
        }
    };

    let plan = cluaiz_plan::Planner::create_plan(&model_id, &geometry, &hw, &settings);

    let model_size_gb = cluaiz_core::bytes_to_gib(geometry.file_size_bytes);
    let is_vision = geometry.architecture.contains("vl") || geometry.architecture.contains("vision");
    let vram_free_gb = hw.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
    let vram_total_gb = hw.gpus.first().map(|g| g.vram_total_gb).unwrap_or(0.0);
    let vram_buffer_gb = plan.vram_buffer_gb;
    let usable_vram_gb = (vram_free_gb - vram_buffer_gb).max(0.0);
    let usable_ram_gb = plan.ram_budget_limit_gb;
    let ram_buffer_gb = (hw.memory.available_gb - usable_ram_gb).max(0.0);
    let (gpu_layers, ram_layers) = plan.block_split();
    let placement_mode = format!("{:?}", plan.selected_tier);
    let kv_location = match (plan.kv_gpu_gb > 0.0, plan.kv_ram_gb > 0.0) {
        (true, true) => "vram + ram (next to each layer)",
        (true, false) => "vram",
        (false, true) => "ram",
        (false, false) => "none",
    };
    let remaining_vram = (usable_vram_gb - plan.vram_budget_gb).max(0.0);
    let remaining_ram = (usable_ram_gb - plan.ram_budget_gb).max(0.0);
    let round2 = |v: f64| (v * 100.0).round() / 100.0;

    let gpu = hw.gpus.first();
    let pcie_bus = gpu.and_then(|g| match (g.pcie_gen, g.pcie_width) {
        (Some(gen), Some(width)) => Some(format!("PCIe Gen {}.0 x{}", gen, width)),
        _ => None,
    });
    let pcie_bw = gpu.map(|g| g.pcie_bandwidth_gbps).filter(|b| *b > 0.0);
    let ssd_speed = hw.storage.first().map(|s| s.read_speed_mbps).filter(|v| *v > 0.0);

    Json(json!({
        "status": "success",
        "active_user_settings": {
            "batch_size": settings.optimization.resolved_batch_size(),
            "ubatch_size": settings.optimization.resolved_ubatch_size(),
            "flash_attention": format!("{:?}", settings.optimization.flash_attention),
            "kv_cache_quantization": format!("{:?}", settings.optimization.kv_cache_quantization),
            "kv_cache_type_used": format!("{:?}", plan.kv_cache_type),
            "n_ctx": plan.context_window,
            "n_gpu_layers": plan.n_gpu_layers,
            "ram_buffer_gb": ram_buffer_gb,
            "vram_buffer_gb": vram_buffer_gb,
            "target_slot": "chat_slot"
        },
        "execution_plan": {
            "placement_mode": placement_mode,
            "n_gpu_layers": plan.n_gpu_layers,
            "n_cpu_moe": plan.n_cpu_moe,
            "offload_kqv": plan.kv_offload,
            "gpu_layers": gpu_layers,
            "ram_layers": ram_layers,
            "disk_stream_gb": round2(plan.disk_stream_gb),
            "kv_cache_location": kv_location,
            "model_memory_breakdown": {
                "vram_gb": round2(plan.vram_budget_gb),
                "ram_gb": round2(plan.ram_budget_gb),
                "kv_cache_vram_gb": round2(plan.kv_gpu_gb),
                "kv_cache_ram_gb": round2(plan.kv_ram_gb),
                "logits_buffer_gb": round2(cluaiz_core::bytes_to_gib(geometry.logits_buffer_bytes(settings.optimization.resolved_ubatch_size()))),
                "remaining_free_vram_gb": round2(remaining_vram),
                "remaining_free_ram_gb": round2(remaining_ram)
            },
            "context_reason": plan.context_reason,
            "decision_reason": plan.explanation,
            // Speed is not measured yet (rule STEP 4): no made-up timings
            "speed": serde_json::Value::Null
        },
        "hardware_info": {
            "cpu": {
                "model": hw.cpu.brand,
                "cores": hw.cpu.physical_cores,
                "threads": hw.cpu.logical_threads,
                "clock_ghz": (hw.cpu.frequency_mhz as f64 / 1000.0 * 100.0).round() / 100.0,
                "peak_gflops": (hw.cpu.gflops_fp32 * 10.0).round() / 10.0,
                "current_load_pct": serde_json::Value::Null
            },
            "gpu": {
                "model": hw.gpus.first().map(|g| g.name.clone()).unwrap_or_else(|| "None".to_string()),
                "total_vram_gb": (vram_total_gb * 100.0).round() / 100.0,
                "free_vram_gb": (vram_free_gb * 100.0).round() / 100.0,
                "usable_vram_gb": (usable_vram_gb * 100.0).round() / 100.0,
                "safety_buffer_gb": vram_buffer_gb,
                "vram_speed_gbps": serde_json::Value::Null,
                "compute_capability": gpu.and_then(|g| g.compute_capability.clone())
            },
            "gpu_pcie_bus": {
                "bus_type": pcie_bus,
                "transfer_speed_gbps": pcie_bw,
                "bus_latency_us": serde_json::Value::Null,
                "description": "High-speed connection highway between System RAM and GPU VRAM"
            },
            "ram": {
                "total_ram_gb": (hw.memory.total_gb * 100.0).round() / 100.0,
                "free_ram_gb": (hw.memory.available_gb * 100.0).round() / 100.0,
                "usable_ram_gb": (usable_ram_gb * 100.0).round() / 100.0,
                "safety_buffer_gb": ram_buffer_gb,
                "read_speed_gbps": (hw.memory.bandwidth_gbps * 10.0).round() / 10.0,
                "access_latency_ns": (hw.memory.memory_latency_ns * 10.0).round() / 10.0
            },
            "storage_ssd": {
                "drive_type": hw.storage.first().map(|s| s.drive_type.clone()),
                "speed_mbps": ssd_speed
            },
            "workspace_buffer": {
                "logits_buffer_gb": round2(cluaiz_core::bytes_to_gib(geometry.logits_buffer_bytes(settings.optimization.resolved_ubatch_size()))),
                "purpose": "llama.cpp logits part of the compute buffer (vocab x ubatch x f32)"
            }
        },
        "model_info": {
            "model_id": model_id,
            "architecture": geometry.architecture,
            "total_layers": geometry.block_count,
            "active_working_context": plan.context_window,
            "max_input_context": geometry.context_length,
            "max_output_context": plan.context_window,
            "model_size_gb": (model_size_gb * 100.0).round() / 100.0,
            "attention_heads": geometry.head_count,
            "kv_heads": geometry.head_count_kv,
            "head_dim": geometry.head_dim(),
            "inputs": if is_vision { vec!["text", "image"] } else { vec!["text"] },
            "outputs": vec!["text"],
            "supported_attachments": if is_vision {
                vec!["image", "pdf", "video", "txt"]
            } else {
                vec!["pdf", "txt", "code", "markdown", "csv", "json"]
            }
        },
        "candidates": plan.candidates
    }))
}

/// GGUF settings page payload. Every field maps to a real engine setting in settings.json;
/// a missing field keeps the saved value. llama.cpp meanings: n_gpu_layers -1 = Auto, n_ctx 0 = Auto / -1 = Max.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct GgufHardwareExecution {
    pub n_gpu_layers: Option<i32>,
    pub n_ctx: Option<i64>,
    pub no_mmap: Option<bool>,
    pub batch_size: Option<u32>,
    pub ubatch_size: Option<u32>,
}

/// Default samplers; null = the model's own default (then llama.cpp's).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct GgufSamplers {
    pub temp: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub min_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub repeat_penalty: Option<f32>,
    pub seed: Option<u32>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct UserMovedFlags {
    pub think_mode: Option<cluaiz_core::FeatureState>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct GgufMetadataHeadersPayload {
    pub hardware_and_execution: Option<GgufHardwareExecution>,
    pub samplers: Option<GgufSamplers>,
    pub user_moved_flags: Option<UserMovedFlags>,
}

// ─── GET /v1/system/gguf_config & POST /v1/system/gguf_config ────────
// Only settings the engine uses; all of them live in settings.json.
pub async fn get_gguf_config() -> Json<Value> {
    let settings = Settings::current();
    let opt = &settings.optimization;
    Json(json!({
        "status": "success",
        // The saved values, null = automatic (not the resolved defaults: saving the page back must not turn
        // "automatic" into fixed numbers)
        "hardware_and_execution": {
            "n_gpu_layers": opt.n_gpu_layers.unwrap_or(-1),
            "n_ctx": opt.context_setting().to_llama_n_ctx(),
            "no_mmap": opt.use_mmap.map(|m| !m),
            "batch_size": opt.batch_size,
            "ubatch_size": opt.ubatch_size
        },
        "samplers": settings.samplers,
        "user_moved_flags": {
            "think_mode": opt.think_mode
        }
    }))
}

pub async fn update_gguf_config(Json(payload): Json<Value>) -> (StatusCode, Json<Value>) {
    match tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let mut current = Settings::current();
        let parsed: GgufMetadataHeadersPayload = serde_json::from_value(payload)?;
        if let Some(hw) = parsed.hardware_and_execution {
            let opt = &mut current.optimization;
            if let Some(n) = hw.n_gpu_layers {
                // -1 = Auto (planner decides), 0 = CPU only, N = exactly N layers
                opt.n_gpu_layers = if n < 0 { None } else { Some(n) };
            }
            if let Some(n_ctx) = hw.n_ctx {
                opt.set_context_setting(cluaiz_core::ContextSetting::from_llama_n_ctx(n_ctx));
            }
            if let Some(no_mmap) = hw.no_mmap {
                opt.use_mmap = Some(!no_mmap);
            }
            if let Some(b) = hw.batch_size {
                opt.batch_size = Some(b);
            }
            if let Some(ub) = hw.ubatch_size {
                opt.ubatch_size = Some(ub);
            }
        }
        if let Some(sp) = parsed.samplers {
            current.samplers = cluaiz_core::SamplerDefaults {
                temp: sp.temp,
                top_p: sp.top_p,
                top_k: sp.top_k,
                min_p: sp.min_p,
                presence_penalty: sp.presence_penalty,
                frequency_penalty: sp.frequency_penalty,
                repeat_penalty: sp.repeat_penalty,
                seed: sp.seed,
            };
        }
        if let Some(think_mode) = parsed.user_moved_flags.and_then(|f| f.think_mode) {
            current.optimization.think_mode = think_mode;
        }
        Ok(Settings::update(current)?)
    }).await {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Ok(Err(e)) => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "status": "error", "message": e.to_string() }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "status": "error", "message": format!("Blocking task failed: {}", e) }))),
    }
}

// ─── GET /v1/system/onnx_config & POST /v1/system/onnx_config ────────
pub async fn get_onnx_config() -> Json<Value> {
    Json(serde_json::to_value(Settings::current().onnx).unwrap_or_default())
}

/// Partial update of the ONNX section of settings.json; unknown keys are rejected
pub async fn update_onnx_config(Json(patch): Json<Value>) -> (StatusCode, Json<Value>) {
    let mut current = Settings::current();
    current.onnx = match current.onnx.merged_with(&patch) {
        Ok(onnx) => onnx,
        Err(e) => {
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "status": "error", "message": format!("Invalid ONNX setting: {}", e) })));
        }
    };
    match Settings::update(current) {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "status": "error", "message": e.to_string() }))),
    }
}

// ─── POST /v1/system/cmd (Secure Local Terminal Execution) ───────────
#[derive(serde::Deserialize)]
pub struct CmdPayload {
    pub command: String,
}

pub async fn execute_cmd(
    Json(payload): Json<CmdPayload>,
) -> Json<Value> {
    let cmd_trimmed = payload.command.trim();
    if cmd_trimmed.is_empty() {
        return Json(json!({ "status": "error", "output": "Command payload cannot be empty." }));
    }
    if cmd_trimmed.len() > 8192 {
        return Json(json!({ "status": "error", "output": "Command payload exceeds maximum permitted length of 8192 characters." }));
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
        Err(e) => Json(json!({ "status": "error", "output": format!("Failed to execute process: {}", e) })),
    }
}
