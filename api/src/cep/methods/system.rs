//! `system.*`: the Engine and the machine it runs on.

use crate::cep::{roots, CallCtx};
use cluaiz_cep::{Call, CepError, Meta, NoParams, Registry, Scope};
use cluaiz_core::{ActiveUserSettings, LlamaLoadParams, Settings};
use cluaiz_hw::HardwareProber;
use schemars::JsonSchema;
use serde_json::{json, Value};

pub fn register(registry: &mut Registry<CallCtx>) {
    registry.add(
        "system.info",
        Meta::new(Scope::Read, "Engine name and version, CEP version, hardware, and the LAN address").example(
            json!({}),
            json!({ "engine": "Cluaiz Engine", "version": "0.1.0", "cep_version": 1,
                    "hardware": { "architecture": "x86_64", "os_name": "Windows", "os_version": "11",
                                  "cpu": { "brand": "AMD Ryzen 7 7435HS", "physical_cores": 8, "logical_threads": 16 },
                                  "memory": { "total_gb": 23.69, "available_gb": 7.47 },
                                  "gpus": [{ "index": 0, "vendor": "NVIDIA", "name": "NVIDIA GeForce RTX 3050 A Laptop GPU", "vram_total_gb": 4.0, "compute_capability": "8.9" }] },
                    "network": { "lan_ip": "192.168.1.20" } }),
        ),
        |_: CallCtx, _: Call, _: NoParams| async move { Ok(info()) },
    );
    registry.add(
        "system.live",
        Meta::new(Scope::Read, "Current process, memory, CPU and GPU telemetry")
            .example(json!({}), json!({"process_id":18356,"memory":{"total_gb":23.69,"available_gb":7.47,"used_gb":16.23},
                "cpu":{"brand":"AMD Ryzen 7 7435HS","physical_cores":8,"logical_threads":16,"frequency_mhz":2002},
                "gpus":[{"index":0,"vendor":"NVIDIA","name":"NVIDIA GeForce RTX 3050 A Laptop GPU","vram_total_bytes":4292870144_u64,
                         "vram_free_bytes":2816151552_u64,"compute_capability":"8.9","pcie_gen":2,"pcie_width":8}]})),
        |_: CallCtx, _: Call, _: NoParams| async move { Ok(live()) },
    );
    registry.add(
        "system.ps",
        Meta::new(
            Scope::Read,
            "Loaded model execution plan and hardware snapshot",
        )
        .example(
            json!({}),
            json!({"active_processes":[{"pid":"18356","model_id":"qwen3-4b-gguf-Q4_K_M","vram_gb":3.2,"context_size":8192,
                                         "tier":"Gpu","n_gpu_layers":37,"engine":"llama-engine (static rlib)"}],
                   "hardware_snapshot":{"architecture":"x86_64","cpu":{},"gpus":[],"memory":{},"storage":{}}}),
        ),
        |ctx: CallCtx, _: Call, _: NoParams| async move {
            let plan = ctx.state.host.current_plan.lock().await.clone();
            Ok(processes(plan))
        },
    );
    registry.add(
        "system.plan",
        Meta::new(
            Scope::Read,
            "Plans an installed model without loading its weights or changing saved settings",
        )
        .error(-32602, "invalid_params", "`model` is empty, `n_ctx` is below -1, `batch_tokens` is 0, or a buffer is negative")
        .error(-32004, "model_not_found", "No installed model with this id, and no usable `model_path`")
        .error(-32002, "forbidden", "`model_path` is outside the caller's roots (`data.needs: \"roots\"`)")
        .error(-32010, "gguf_probe_failed", "The GGUF header could not be read")
        .error(-32010, "onnx_probe_failed", "The ONNX package could not be inspected")
        .example(
            json!({"model":"installed-model"}),
            json!({"model_info":{"model_id":"bonsai-27b-gguf-q1_0","architecture":"qwen35","context_window":262144,"total_layers":64,
                                 "model_size_gb":3.54,"inputs":[{"type":"text","available":true}],"outputs":["text"]},
                   "execution_plan":{"context_window":4096,"context_reason":"Auto: floor 4096 tokens, KV Kv8; model does not fit on one device",
                                     "gpu_layers":16,"n_gpu_layers":17,"batch_size":512,"kv_cache_type":"Kv8",
                                     "kv_cache_location":"vram + ram (next to each layer)",
                                     "decision_reason":"GPU + RAM split: 16 / 64 blocks + output head on GPU (1.57 GB), 48 blocks in RAM",
                                     "model_memory_breakdown":{"vram_gb":1.57,"ram_gb":2.96,"kv_cache_vram_gb":0.13,"kv_cache_ram_gb":0.4}},
                   "active_user_settings":{"flash_attention":"Auto","kv_cache_quantization":"Auto"},
                   "hardware_info":{},"candidates":[]}),
        ),
        |ctx: CallCtx, call: Call, payload: PlanRequest| async move {
            plan_execution(ctx, call, payload).await
        },
    );
}

fn live() -> Value {
    let hw = HardwareProber::current();
    json!({
        "process_id":std::process::id(),
        "memory":{"total_gb":hw.memory.total_gb,"available_gb":hw.memory.available_gb,
                  "used_gb":(hw.memory.total_gb-hw.memory.available_gb).max(0.0)},
        "cpu":{"brand":hw.cpu.brand,"physical_cores":hw.cpu.physical_cores,
               "logical_threads":hw.cpu.logical_threads,"frequency_mhz":hw.cpu.frequency_mhz},
        "gpus":hw.gpus
    })
}

fn processes(plan: Option<cluaiz_plan::ExecutionPlan>) -> Value {
    let active: Vec<Value> = plan
        .into_iter()
        .map(|plan| {
            json!({
                "pid":std::process::id().to_string(), "model_id":plan.model_id,
                "vram_gb":plan.vram_budget_gb, "context_size":plan.context_window,
                "tier":format!("{:?}",plan.selected_tier), "n_gpu_layers":plan.n_gpu_layers,
                "engine":"llama-engine (static rlib)"
            })
        })
        .collect();
    json!({"active_processes":active,"hardware_snapshot":HardwareProber::current()})
}

fn info() -> Value {
    json!({
        "engine": "Cluaiz Engine",
        "version": env!("CARGO_PKG_VERSION"),
        "cep_version": cluaiz_cep::discover::CEP_VERSION,
        "hardware": HardwareProber::current(),
        "network": { "lan_ip": lan_ip() },
    })
}

/// The address other devices on the network reach this machine at: the local side of a UDP
/// socket pointed at a public address (connect sends no packet)
pub fn lan_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

// Model execution planning (`system.plan`).
#[derive(serde::Deserialize, serde::Serialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    /// An installed model id (from `models.list`)
    pub model: String,
    /// Plan this weight file instead of the model's primary file (it must pass the caller's roots)
    pub model_path: Option<String>,
    /// Logical batch size in tokens for the plan (llama.cpp `-b`; default: the saved setting)
    pub batch_tokens: Option<u32>,
    /// Context size: -1 = the model's maximum, 0 = automatic, N = N tokens (default: the saved setting)
    pub n_ctx: Option<i64>,
    /// VRAM to keep free, in GB (default: the planner's rule)
    pub vram_buffer_gb: Option<f64>,
    /// RAM to keep free, in GB (default: the planner's rule)
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

/// Project intrinsic declarations separately from optional engine preparation routes.
fn chat_input_contract(
    declared_inputs: &[cluaiz_core::Modality],
    video_formats: &[String],
) -> (Vec<Value>, Vec<Value>) {
    use cluaiz_core::Modality;
    let formats = cluaiz_media::formats::attachment_formats();
    let inputs = declared_inputs
        .iter()
        .copied()
        .map(|input| {
            // This is positive model capability, not the current session's load state.
            let mut value = json!({"type":input,"available":true});
            if input != Modality::Text {
                value["formats"] = json!(cluaiz_media::formats::unique_extensions(
                    formats
                        .iter()
                        .filter(|(modality, _)| *modality == input)
                        .map(|(_, format)| *format)
                ));
            }
            value
        })
        .collect();
    let preparation = if declared_inputs.contains(&Modality::Image) && !video_formats.is_empty() {
        vec![
            json!({"input":"video","output":"image","processing":"sampled_frames",
            "formats":cluaiz_media::formats::unique_extensions(video_formats.iter().map(String::as_str))}),
        ]
    } else {
        Vec::new()
    };
    (inputs, preparation)
}

async fn plan_execution(ctx: CallCtx, call: Call, payload: PlanRequest) -> Result<Value, CepError> {
    let state = &ctx.state;
    let saved = Settings::current();
    let settings = payload
        .apply(&saved)
        .map_err(|message| roots::invalid("params", &message))?;
    let model_id = &payload.model;
    let hw = HardwareProber::with_live_memory();

    // Resolve real model geometry from provided path or InstalledStateRegistry
    let entry = cluaiz_models::registry::InstalledStateRegistry::find(model_id);
    let model_file_path = payload
        .model_path
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| entry.as_ref().and_then(|entry| entry.primary_path()));
    let selected_path = model_file_path.ok_or_else(|| {
        CepError::not_found(
            "model_not_found",
            format!("Model '{model_id}' is not installed and no valid model_path was provided"),
        )
    })?;
    let workspace = state.current_workspace_dir.read().await.clone();
    let model_file_path = roots::resolve(&selected_path.to_string_lossy(), &workspace, false)
        .map_err(|error| {
            if error.code == cluaiz_cep::error::NOT_FOUND {
                CepError::not_found("model_not_found", error.message)
            } else {
                error
            }
        })?;
    call.check_path(&model_file_path)?;
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
            Err(error) => {
                return Err(CepError::unavailable(
                    "onnx_probe_failed",
                    error.to_string(),
                ))
            }
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
            return Err(CepError::unavailable(
                "package_size_overflow",
                "Model package size overflow",
            ));
        };
        return Ok(json!({
            "model_info":{"model_id":model_id,"backend":"onnx","architecture":entry.map(|e| &e.metadata.architecture),
                "inputs":support.inputs,"available_inputs":support.available_inputs,"input_availability":support.input_availability,
                "outputs":outputs,"supported_attachments":support.supported_attachments,
                "task_routes":support.task_routes,"missing_assets":support.missing_assets,"package_capabilities":package,
                "model_size_gb":cluaiz_core::bytes_to_gib(bytes),"active_working_context":null,"max_input_context":null,"max_output_context":null},
            "execution_plan":{"backend":"onnx","placement_mode":"runtime_provider_selection",
                "provider_policy":if settings.onnx.n_gpu_layers == 0 { "cpu" } else { "cuda_then_windows_directml_then_cpu" },
                "selected_provider":null,"peak_memory_gb":null,"speed":null,
                "decision_reason":"Graph/helper metadata is inspected without loading weights. Native task readiness requires a successful retained request."},
            "active_user_settings":{"onnx":settings.onnx},"hardware_info":hw}));
    }

    let header = match cluaiz_models::GgufProber::read_header(&model_file_path) {
        Ok(header) => header,
        Err(error) => {
            return Err(CepError::unavailable(
                "gguf_probe_failed",
                format!("Failed to probe GGUF geometry: {error}"),
            ))
        }
    };
    let geometry = match cluaiz_models::GgufProber::geometry_from_header(&model_file_path, &header)
    {
        Ok(geometry) => geometry,
        Err(error) => {
            return Err(CepError::unavailable(
                "gguf_probe_failed",
                error.to_string(),
            ))
        }
    };

    let embedding = entry
        .as_ref()
        .is_some_and(crate::handlers::embeddings::is_embedding_model);
    let settings = if embedding {
        match cluaiz_runtime::embedding_host::capture_settings(&geometry, &settings) {
            Ok(settings) => settings,
            Err(error) => {
                return Err(CepError::unavailable(
                    "embedding_context_invalid",
                    error.to_string(),
                ))
            }
        }
    } else {
        settings
    };
    let plan = cluaiz_plan::Planner::create_plan(model_id, &geometry, &hw, &settings);

    let model_size_gb = cluaiz_core::bytes_to_gib(geometry.file_size_bytes);
    let Some(model_dir) = model_file_path.parent() else {
        return Err(CepError::not_found(
            "model_not_found",
            "Model path has no parent directory",
        ));
    };
    let package = cluaiz_models::capabilities::PackageCapabilities::from_header(
        &model_file_path,
        model_dir,
        &header,
    );
    let declared_inputs = package.supported_inputs();
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

    let video_formats = if declared_inputs.contains(&cluaiz_core::Modality::Image) {
        tokio::task::spawn_blocking(cluaiz_media::video::attachment_formats)
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let (inputs, attachment_processing) = chat_input_contract(&declared_inputs, &video_formats);
    let opt = &saved.optimization;

    let mut response = json!({
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
            "outputs": if embedding { vec!["embedding"] } else { vec!["text"] }
        },
        "candidates": plan.candidates
    });
    if !attachment_processing.is_empty() {
        response["model_info"]["attachment_processing"] = json!(attachment_processing);
    }
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
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_inputs_mark_positive_capabilities_available_without_session_state(
    ) -> anyhow::Result<()> {
        use anyhow::Context;
        use cluaiz_core::Modality;
        let video_formats = vec!["mp4".into(), "m4v".into(), "mp4".into()];
        let (inputs, preparation) = chat_input_contract(&[Modality::Text], &video_formats);
        assert_eq!(inputs, [json!({"type":"text","available":true})]);
        assert!(preparation.is_empty());
        let declared = [Modality::Text, Modality::Image, Modality::Audio];
        let (inputs, preparation) = chat_input_contract(&declared, &video_formats);
        assert_eq!(inputs.len(), 3);
        for input in &inputs {
            assert_eq!(input["available"], true);
            for field in ["support", "reason"] {
                assert!(input.get(field).is_none());
            }
        }
        for (index, aliases) in [(1, ["jpg", "jpeg"]), (2, ["mp2", "mp3"])] {
            let formats = inputs[index]["formats"]
                .as_array()
                .context("Decoder formats")?;
            for alias in aliases {
                assert!(formats.contains(&json!(alias)));
            }
        }
        assert_eq!(
            preparation,
            [
                json!({"input":"video","output":"image","processing":"sampled_frames",
            "formats":["mp4","m4v"]})
            ]
        );
        assert!(chat_input_contract(&declared, &[]).1.is_empty());
        assert!(chat_input_contract(&[], &video_formats).0.is_empty());
        Ok(())
    }

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
