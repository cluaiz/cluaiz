//! 💻 System, Telemetry & Hardware Info Handlers (`api/src/handlers/system.rs`)
//! Modular implementation using cluaiz-hw HardwareProber, cluaiz-plan Planner, and cluaiz-core Settings.

use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use cluaiz_core::{ActiveUserSettings, LlamaLoadParams, Settings};
use cluaiz_hw::HardwareProber;
use serde_json::{json, Value};
use std::sync::Arc;

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
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub model: String,
    pub model_path: Option<String>,
    pub batch_tokens: Option<u32>,
    pub n_ctx: Option<i64>,
    pub vram_buffer_gb: Option<f64>,
    pub ram_buffer_gb: Option<f64>,
}

impl PlanRequest {
    fn apply(&self, saved: &ActiveUserSettings) -> Result<ActiveUserSettings, String> {
        if self.model.trim().is_empty() {
            return Err("model must name an installed model".into());
        }
        if self.n_ctx.is_some_and(|n| n < -1 || n > u32::MAX as i64) {
            return Err("n_ctx must be -1 (Max), 0 (Auto), or a positive u32 token count".into());
        }
        if self.batch_tokens == Some(0) {
            return Err("batch_tokens must be positive".into());
        }
        for (field, value) in [
            ("ram_buffer_gb", self.ram_buffer_gb),
            ("vram_buffer_gb", self.vram_buffer_gb),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(format!("{field} must be a finite nonnegative number"));
            }
        }
        let mut settings = saved.clone();
        LlamaLoadParams {
            n_ctx: self.n_ctx,
            n_batch: self.batch_tokens,
            vram_buffer_gb: self.vram_buffer_gb,
            ..Default::default()
        }
        .apply_to(&mut settings.optimization)?;
        if let Some(buffer) = self.ram_buffer_gb {
            settings.optimization.custom_ram_buffer_gb = Some(buffer);
        }
        Ok(settings)
    }
}

pub async fn plan_execution(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PlanRequest>,
) -> Response {
    let saved = Settings::current();
    let settings = match payload.apply(&saved) {
        Ok(settings) => settings,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status":"error","code":"invalid_plan_input","message":message})),
            )
                .into_response()
        }
    };
    let model_id = &payload.model;
    let hw = HardwareProber::with_live_memory();

    // Resolve real model geometry from provided path or InstalledStateRegistry
    let entry = cluaiz_models::registry::InstalledStateRegistry::find(model_id);
    let model_file_path = if let Some(ref path) = payload.model_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            Some(p)
        } else {
            None
        }
    } else {
        entry.as_ref().and_then(|entry| entry.primary_path())
    };
    let model_file_path = match model_file_path {
        Some(path) => match path.canonicalize() {
            Ok(path) => path,
            Err(error) => return Json(json!({"status":"error","code":"model_not_found","message":error.to_string()})).into_response(),
        },
        None => {
            return Json(json!({"status":"error","code":"model_not_found",
            "message":format!("Model '{model_id}' is not installed and no valid model_path was provided")})).into_response()
        }
    };
    if model_file_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("onnx"))
    {
        // Keep the existing ONNX planning contract and full registry refresh for that branch.
        let registry = cluaiz_models::registry::InstalledStateRegistry::load();
        let entry = entry
            .as_ref()
            .and_then(|entry| registry.installed_models.get(&entry.id));
        let canonical = model_file_path.canonicalize().ok();
        let cached = entry
            .and_then(|entry| entry.metadata.capabilities.as_ref())
            .filter(|package| canonical.as_ref() == Some(&package.primary_file))
            .cloned();
        // Registry refresh already checked the primary and complete asset snapshot.
        let package = match cached.map(Ok).unwrap_or_else(|| {
            cluaiz_models::capabilities::PackageCapabilities::inspect(&model_file_path)
        }) {
            Ok(package) => package,
            Err(error) => return Json(
                json!({"status":"error","code":"onnx_probe_failed","message":error.to_string()}),
            )
            .into_response(),
        };
        let loaded = match entry.filter(|e| {
            e.primary_path()
                .and_then(|p| p.canonicalize().ok())
                .as_ref()
                == Some(&package.primary_file)
        }) {
            Some(entry) => state.host.onnx.readiness(entry).await.ok().flatten(),
            None => None,
        };
        let formats = cluaiz_media::formats::attachment_formats();
        let routes: Vec<_> = formats
            .iter()
            .map(
                |&(input, format)| cluaiz_runtime::media_support::AttachmentRoute { format, input },
            )
            .collect();
        let support =
            cluaiz_runtime::media_support::resolve_onnx_support(&package, loaded.as_ref(), &routes);
        let outputs: std::collections::BTreeSet<_> = support
            .task_routes
            .iter()
            .flat_map(|route| &route.outputs)
            .collect();
        let Some(bytes) = package
            .asset_snapshot
            .iter()
            .try_fold(0u64, |total, asset| total.checked_add(asset.bytes))
        else {
            return Json(json!({"status":"error","code":"package_size_overflow"})).into_response();
        };
        return Json(json!({"status":"success",
            "model_info":{"model_id":model_id,"backend":"onnx","architecture":entry.map(|e| &e.metadata.architecture),
                "inputs":support.inputs,"available_inputs":support.available_inputs,"input_availability":support.input_availability,
                "outputs":outputs,"supported_attachments":support.supported_attachments,
                "task_routes":support.task_routes,"missing_assets":support.missing_assets,"package_capabilities":package,
                "model_size_gb":cluaiz_core::bytes_to_gib(bytes),"active_working_context":null,"max_input_context":null,"max_output_context":null},
            "execution_plan":{"backend":"onnx","placement_mode":"runtime_provider_selection",
                "provider_policy":if settings.onnx.n_gpu_layers == 0 { "cpu" } else { "cuda_then_windows_directml_then_cpu" },
                "selected_provider":null,"peak_memory_gb":null,"speed":null,
                "decision_reason":"Graph/helper metadata is inspected without loading weights. Native task readiness requires a successful retained request."},
            "active_user_settings":{"onnx":settings.onnx},"hardware_info":hw})).into_response();
    }

    let header = match cluaiz_models::GgufProber::read_header(&model_file_path) {
        Ok(header) => header,
        Err(error) => {
            return Json(json!({"status": "error", "code": "gguf_probe_failed",
            "message": format!("Failed to probe GGUF geometry: {error}")}))
            .into_response()
        }
    };
    let geometry =
        match cluaiz_models::GgufProber::geometry_from_header(&model_file_path, &header) {
            Ok(geometry) => geometry,
            Err(error) => return Json(
                json!({"status":"error","code":"gguf_probe_failed","message":error.to_string()}),
            )
            .into_response(),
        };

    let plan = cluaiz_plan::Planner::create_plan(model_id, &geometry, &hw, &settings);

    let model_size_gb = cluaiz_core::bytes_to_gib(geometry.file_size_bytes);
    let Some(model_dir) = model_file_path.parent() else {
        return Json(json!({"status":"error","code":"model_not_found"})).into_response();
    };
    let package = cluaiz_models::capabilities::PackageCapabilities::from_header(
        &model_file_path,
        model_dir,
        &header,
    );
    let support = state.host.input_support(&package);
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

    let formats = cluaiz_media::formats::attachment_formats();
    let mut inputs: Vec<_> = support.input_availability.iter().map(|input| {
        let mut value = json!({"type":input.input,"support":input.model_support,"available":input.available});
        if let Some(reason) = input.reason { value["reason"] = json!(reason); }
        if input.model_support == cluaiz_models::capabilities::CapabilityState::Supported
            && input.input != cluaiz_core::Modality::Text {
            value["formats"] = json!(cluaiz_media::formats::canonical_extensions(
                formats.iter().filter(|(modality, _)| *modality == input.input).map(|(_, format)| *format)
            ));
        }
        value
    }).collect();
    if package
        .supported_inputs()
        .contains(&cluaiz_core::Modality::Image)
    {
        let video_formats = tokio::task::spawn_blocking(cluaiz_media::video::attachment_formats)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let mut video = json!({"type":"video","support":"supported","processing":"sampled_frames",
            "available":support.available_inputs.contains(&cluaiz_core::Modality::Image) && !video_formats.is_empty()});
        if video_formats.is_empty() {
            video["reason"] = json!("video_decoder_unavailable");
        } else {
            video["formats"] = json!(cluaiz_media::formats::canonical_extensions(
                video_formats.iter().map(String::as_str)
            ));
            if !support
                .available_inputs
                .contains(&cluaiz_core::Modality::Image)
            {
                video["reason"] = json!("image_input_not_ready");
            }
        }
        inputs.push(video);
    }
    let opt = &saved.optimization;

    let mut response = json!({
        "status": "success",
        "active_user_settings": {
            "batch_size": opt.batch_size, "ubatch_size": opt.ubatch_size,
            "flash_attention": opt.flash_attention, "kv_cache_quantization": opt.kv_cache_quantization,
            "custom_context_window": opt.custom_context_window, "n_gpu_layers": opt.n_gpu_layers,
            "custom_ram_buffer_gb": opt.custom_ram_buffer_gb, "custom_vram_buffer_gb": opt.custom_vram_buffer_gb,
            "n_cpu_moe": opt.n_cpu_moe, "offload_kqv": opt.offload_kqv, "op_offload": opt.op_offload,
            "use_mmap": opt.use_mmap, "n_threads": opt.n_threads, "n_threads_batch": opt.n_threads_batch
        },
        "execution_plan": {
            "placement_mode": placement_mode,
            "context_window": plan.context_window,
            "batch_size": settings.optimization.resolved_batch_size(),
            "ubatch_size": settings.optimization.resolved_ubatch_size(),
            "kv_cache_type": plan.kv_cache_type,
            "use_mmap": plan.use_mmap(&settings),
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
            "decision_reason": plan.explanation
        },
        "hardware_info": {
            "cpu": {
                "model": hw.cpu.brand,
                "cores": hw.cpu.physical_cores,
                "threads": hw.cpu.logical_threads
            },
            "gpu": {
                "model": hw.gpus.first().map(|g| g.name.clone()).unwrap_or_else(|| "None".to_string()),
                "total_vram_gb": (vram_total_gb * 100.0).round() / 100.0,
                "free_vram_gb": (vram_free_gb * 100.0).round() / 100.0,
                "usable_vram_gb": (usable_vram_gb * 100.0).round() / 100.0,
                "safety_buffer_gb": vram_buffer_gb
            },
            "ram": {
                "total_ram_gb": (hw.memory.total_gb * 100.0).round() / 100.0,
                "free_ram_gb": (hw.memory.available_gb * 100.0).round() / 100.0,
                "usable_ram_gb": (usable_ram_gb * 100.0).round() / 100.0,
                "safety_buffer_gb": ram_buffer_gb
            }
        },
        "model_info": {
            "model_id": model_id,
            "architecture": geometry.architecture,
            "total_layers": geometry.block_count,
            "context_window": geometry.context_length,
            "model_size_gb": (model_size_gb * 100.0).round() / 100.0,
            "inputs": inputs,
            "outputs": vec!["text"]
        },
        "candidates": plan.candidates
    });
    let mut overrides = serde_json::Map::new();
    for (key, value) in [
        ("n_ctx", json!(payload.n_ctx)),
        ("batch_tokens", json!(payload.batch_tokens)),
        ("vram_buffer_gb", json!(payload.vram_buffer_gb)),
        ("ram_buffer_gb", json!(payload.ram_buffer_gb)),
    ] {
        if !value.is_null() {
            overrides.insert(key.to_owned(), value);
        }
    }
    if !overrides.is_empty() {
        response["request_overrides"] = Value::Object(overrides);
    }
    Json(response).into_response()
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
        Settings::update(current)
    })
    .await
    {
        Ok(Ok(())) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Ok(Err(e)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": format!("Blocking task failed: {}", e) })),
        ),
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
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(
                    json!({ "status": "error", "message": format!("Invalid ONNX setting: {}", e) }),
                ),
            );
        }
    };
    match Settings::update(current) {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "success" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "status": "error", "message": e.to_string() })),
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_overrides_apply_without_changing_saved_auto_settings() -> anyhow::Result<()> {
        let saved = ActiveUserSettings::default();
        let request: PlanRequest = serde_json::from_value(json!({
            "model":"selected", "n_ctx":1234, "batch_tokens":32,
            "ram_buffer_gb":0.25, "vram_buffer_gb":0.5
        }))?;
        let applied = request.apply(&saved).map_err(anyhow::Error::msg)?;
        assert_eq!(saved.optimization.custom_context_window, None);
        assert_eq!(saved.optimization.batch_size, None);
        assert_eq!(saved.optimization.custom_ram_buffer_gb, None);
        assert_eq!(applied.optimization.custom_context_window, Some(1234));
        assert_eq!(applied.optimization.batch_size, Some(32));
        assert_eq!(applied.optimization.custom_ram_buffer_gb, Some(0.25));
        assert_eq!(applied.optimization.custom_vram_buffer_gb, Some(0.5));
        for (requested, expected) in [(0, None), (-1, Some(0))] {
            let request: PlanRequest =
                serde_json::from_value(json!({"model":"selected","n_ctx":requested}))?;
            assert_eq!(
                request
                    .apply(&saved)
                    .map_err(anyhow::Error::msg)?
                    .optimization
                    .custom_context_window,
                expected
            );
        }
        Ok(())
    }

    #[test]
    fn test_plan_rejects_invalid_or_unused_fields() -> anyhow::Result<()> {
        let saved = ActiveUserSettings::default();
        for patch in [
            json!({"n_ctx":-2}),
            json!({"batch_tokens":0}),
            json!({"ram_buffer_gb":-1}),
            json!({"vram_buffer_gb":-1}),
        ] {
            let mut value = json!({"model":"selected"});
            value
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            let request: PlanRequest = serde_json::from_value(value)?;
            assert!(request.apply(&saved).is_err());
        }
        for field in ["weights_gb", "total_layers", "misspelled_option"] {
            let mut value = json!({"model":"selected"});
            value[field] = json!(1);
            assert!(serde_json::from_value::<PlanRequest>(value).is_err());
        }
        Ok(())
    }
}
