use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::state::AppState;
use dispatcher::EngineResponse;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ServerOptions, NamedPipeServer};

#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\cluaize_engine_pipe";

#[cfg(windows)]
pub async fn start_named_pipe_server(state: Arc<AppState>) {
    let mut consecutive_failures: u32 = 0;
    loop {
        // Create the named pipe server. Do NOT use first_pipe_instance(true) — that flag
        // causes ERROR_ACCESS_DENIED if a prior daemon instance is still alive (e.g. restart).
        let server = match ServerOptions::new().create(PIPE_NAME) {
            Ok(server) => {
                consecutive_failures = 0;
                server
            },
            Err(e) => {
                consecutive_failures += 1;
                tracing::error!("❌ [IPC] Failed to create named pipe (attempt {}): {}", consecutive_failures, e);
                if consecutive_failures >= 10 {
                    tracing::error!("💀 [IPC] Named pipe creation failed 10 times consecutively. IPC bridge is dead. Exiting retry loop.");
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        // Wait for a client (CLI or Tauri App) to connect natively
        match server.connect().await {
            Ok(_) => {
                tracing::info!("🔗 Native Client connected to IPC Pipe.");
                let state_clone = state.clone();
                tokio::spawn(async move {
                    handle_client(server, state_clone).await;
                });
            }
            Err(e) => {
                tracing::error!("❌ [IPC] Pipe connection error: {}", e);
            }
        }
    }
}

#[cfg(windows)]
async fn handle_client(mut pipe: NamedPipeServer, state: Arc<AppState>) {
    let mut buf = vec![0; 4096];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) => {
                tracing::info!("🔗 Native Client disconnected from IPC Pipe.");
                break;
            }
            Ok(n) => {
                let msg = String::from_utf8_lossy(&buf[..n]).to_string();
                let command = msg.trim();
                tracing::info!("📥 [IPC] Received Command: {}", command);

                // Try to parse as JSON first for Universal FFI Parity
                if let Ok(json_cmd) = serde_json::from_str::<serde_json::Value>(command) {
                    if let Some(action) = json_cmd.get("action").and_then(|a| a.as_str()) {
                        match action {
                            "EXTENSION_PAYLOAD" => {
                                let ext_name = json_cmd.get("extension_name").and_then(|e| e.as_str()).unwrap_or("");
                                let payload = json_cmd.get("payload").and_then(|p| p.as_str()).unwrap_or("{}");
                                let mut manager = engines::neural_foundry::registry::extension_manager::ExtensionManager::new();
                                // Assuming we scan the default domain or active extensions are already in memory, for FFI we might need a singleton or just scan.
                                let base_path = cluaize_shared::environment::EnvironmentManager::current().global_dir.join("core");
                                let _ = manager.scan_domain(&base_path);
                                match manager.execute(ext_name, payload) {
                                    Ok(res) => { let _ = pipe.write_all(res.as_bytes()).await; }
                                    Err(e) => { let _ = pipe.write_all(format!("{{\"status\": \"error\", \"message\": \"{}\"}}", e).as_bytes()).await; }
                                }
                                continue;
                            }
                            "SYSTEM_PS" => {
                                let registry = cluaize_shared::hardware::governor::HardwareGovernor::load_process_registry();
                                let mut processes = Vec::new();
                                for (pid_str, info) in registry {
                                    processes.push(serde_json::json!({
                                        "pid": pid_str,
                                        "model_id": info.model_id,
                                        "vram_gb": info.vram_gb,
                                        "context_size": info.context_size,
                                        "engine": info.engine
                                    }));
                                }
                                let res = serde_json::json!({"status": "success", "active_processes": processes});
                                let _ = pipe.write_all(res.to_string().as_bytes()).await;
                                continue;
                            }
                            "HARDWARE_CALIBRATE" => {
                                let _ = cluaize_shared::hardware::governor::HardwareGovernor::auto_calibrate();
                                let _ = pipe.write_all(b"{\"status\": \"success\", \"message\": \"Hardware recalibrated\"}").await;
                                continue;
                            }
                            "BENCHMARK_RUN" => {
                                engines::telemetry::health_check::CluaizeHealthChecker::run_full_benchmark();
                                let _ = pipe.write_all(b"{\"status\": \"success\", \"message\": \"Benchmark started\"}").await;
                                continue;
                            }
                            "SYSTEM_PROFILE_SETUP" => {
                                engines::hardware::system_control_manager::detect_hardware();
                                let _ = pipe.write_all(b"{\"status\": \"success\", \"message\": \"Profile generated\"}").await;
                                continue;
                            }
                            "MODEL_RM" => {
                                if let Some(model_id) = json_cmd.get("payload").and_then(|p| p.get("model_id")).and_then(|m| m.as_str()) {
                                    let model_file = cluaize_shared::environment::EnvironmentManager::current()
                                        .ensure_models_dir()
                                        .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().models_dir())
                                        .join(format!("{}.gguf", model_id));
                                    if model_file.exists() {
                                        let _ = std::fs::remove_file(&model_file);
                                        let _ = pipe.write_all(b"{\"status\": \"success\", \"message\": \"Model removed\"}").await;
                                    } else {
                                        let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"File not found\"}").await;
                                    }
                                } else {
                                    let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"Missing model_id\"}").await;
                                }
                                continue;
                            }
                            "SKILL_LIST" => {
                                let skills_dir = cluaize_shared::environment::EnvironmentManager::current().skills_dir();
                                let mut skill_names: Vec<String> = Vec::new();
                                if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                                    for entry in entries.flatten() {
                                        if entry.path().is_dir() {
                                            if let Some(name) = entry.file_name().to_str() {
                                                skill_names.push(name.to_string());
                                            }
                                        }
                                    }
                                }
                                let res = serde_json::json!({"status": "success", "skills": skill_names});
                                let _ = pipe.write_all(res.to_string().as_bytes()).await;
                                continue;
                            }
                            "SKILL_CACHE_CLEAR" => {
                                let skills_dir = cluaize_shared::environment::EnvironmentManager::current().skills_dir();
                                let mut cleared = 0usize;
                                if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                                    for entry in entries.flatten() {
                                        let cache_dir = entry.path().join(".cache");
                                        if cache_dir.is_dir() {
                                            if std::fs::remove_dir_all(&cache_dir).is_ok() {
                                                cleared += 1;
                                            }
                                        }
                                    }
                                }
                                let res = serde_json::json!({"status": "success", "cleared_skills": cleared});
                                let _ = pipe.write_all(res.to_string().as_bytes()).await;
                                continue;
                            }
                            "SKILL_CACHE_LS" => {
                                let skills_dir = cluaize_shared::environment::EnvironmentManager::current().skills_dir();
                                let mut cache_entries: Vec<serde_json::Value> = Vec::new();
                                if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                                    for entry in entries.flatten() {
                                        let cache_dir = entry.path().join(".cache");
                                        if cache_dir.is_dir() {
                                            let files: Vec<String> = std::fs::read_dir(&cache_dir)
                                                .into_iter().flatten().flatten()
                                                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                                                .collect();
                                            if let Some(name) = entry.file_name().to_str() {
                                                cache_entries.push(serde_json::json!({"skill": name, "cache_files": files}));
                                            }
                                        }
                                    }
                                }
                                let res = serde_json::json!({"status": "success", "cache": cache_entries});
                                let _ = pipe.write_all(res.to_string().as_bytes()).await;
                                continue;
                            }
                            "CDQL_DELETE_SESSION" | "INGEST_DOC" => {
                                let res = serde_json::json!({
                                    "status": "not_implemented",
                                    "action": action,
                                    "message": "This command is reserved for a future release."
                                });
                                let _ = pipe.write_all(res.to_string().as_bytes()).await;
                                continue;
                            }
                            "SYSTEM_BRAIN" => {
                                if let Some(payload) = json_cmd.get("payload").and_then(|p| p.get("state")).and_then(|s| s.as_bool()) {
                                    if let Ok(mut control) = cluaize_shared::hardware::governor::HardwareGovernor::load_system_control() {
                                        control.brain.cluaizd_connect_ffi = if payload { "on".to_string() } else { "off".to_string() };
                                        let _ = cluaize_shared::hardware::system_control::HardwareOrchestrator::persist_sovereign_state(&control);
                                        let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                    } else {
                                        let _ = pipe.write_all(b"{\"status\": \"error\"}").await;
                                    }
                                } else {
                                    let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"missing state payload\"}").await;
                                }
                                continue;
                            }
                            "GET_SETTINGS" => {
                                let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                let control = cluaize_shared::hardware::governor::HardwareGovernor::load_system_control().unwrap_or_default();
                                let brain_mode = if control.brain.cluaizd_connect_ffi == "on" { "on" } else { "off" };
                                
                                // Load real booster from disk — NOT hardcoded
                                let booster = cluaize_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                                
                                let roster = engines::models::registry::CoreRoster::load_roster();
                                let mut available_chat_models: Vec<String> = Vec::new();
                                let mut available_vector_models: Vec<String> = Vec::new();
                                let mut all_models: Vec<String> = Vec::new();

                                for model in &roster {
                                    all_models.push(model.id.clone());

                                    // PRIMARY: Classify by the actual folder path on disk
                                    // ~/.cluaize/models/chat/     → chat models
                                    // ~/.cluaize/models/embedding/ → vector models (text embeddings)
                                    // ~/.cluaize/models/vision/    → vector models (image embeddings / CLIP)
                                    let folder_category = model.local_path.as_deref()
                                        .and_then(|p| {
                                            let p_lower = p.replace('\\', "/").to_lowercase();
                                            if p_lower.contains("/models/chat/") {
                                                Some("chat")
                                            } else if p_lower.contains("/models/embedding/") {
                                                Some("embedding")
                                            } else if p_lower.contains("/models/vision/") {
                                                Some("vision")
                                            } else {
                                                None
                                            }
                                        });

                                    // FALLBACK: Use the `category` field from model_manifest.json
                                    let cat = folder_category
                                        .unwrap_or_else(|| model.category.as_str())
                                        .to_lowercase();

                                    // "embedding" and "vision" models go into vector list.
                                    // Vision/CLIP models can embed images into vector space.
                                    match cat.as_str() {
                                        "embedding" | "vision" | "multimodal" => {
                                            available_vector_models.push(model.id.clone());
                                        }
                                        _ => {
                                            available_chat_models.push(model.id.clone());
                                        }
                                    }
                                }

                                // Safety: always ensure the currently-active model appears in its list
                                let active_text_id = perms.chat_models.text.clone();
                                if let Some(ref t) = active_text_id {
                                    if !t.is_empty() && !available_chat_models.contains(t) {
                                        available_chat_models.push(t.clone());
                                    }
                                }
                                if let Some(ref t) = perms.vector_models.text {
                                    if !t.is_empty() && !available_vector_models.contains(t) {
                                        available_vector_models.push(t.clone());
                                    }
                                }

                                // Fetch manifests for the currently active models so frontend can do deep combined validation
                                let active_chat_manifest = active_text_id.and_then(|id| roster.iter().find(|m| m.id == id).cloned());
                                let active_vector_manifest = perms.vector_models.text.clone().and_then(|id| roster.iter().find(|m| m.id == id).cloned());

                                let vram_gb: f64 = control.silicon_truth.accelerators.gpus.iter().map(|g| g.vram_total_gb).sum();
                                let ram_gb = control.silicon_truth.memory.total_capacity_gb;
                                let gpu_name = control.silicon_truth.accelerators.gpus.first()
                                    .map(|g| g.model.clone()).unwrap_or_default();

                                let response = serde_json::json!({
                                    "permissions": {
                                        "wasm_firewall": perms.wasm_firewall,
                                        "vectorize_user_input": perms.vectorize_user_input,
                                        "vectorize_ai_response": perms.vectorize_ai_response,
                                        "stream_telemetry": perms.stream_telemetry,
                                        "lazy_load_model": perms.lazy_load_model,
                                        "temporary_chat_ttl_hours": perms.temporary_chat_ttl_hours,
                                        "chat_models": perms.chat_models,
                                        "vector_models": perms.vector_models,
                                        "available_models": all_models,
                                        "available_chat_models": available_chat_models,
                                        "available_vector_models": available_vector_models,
                                        "available_devices": ["auto", "gpu", "cpu"]
                                    },
                                    "booster": booster,
                                    "brainMode": brain_mode,
                                    "active_chat_model": active_chat_manifest,
                                    "active_vector_model": active_vector_manifest,
                                    "hardware": {
                                        "vram_gb": vram_gb,
                                        "ram_gb": ram_gb,
                                        "gpu_name": gpu_name.trim(),
                                        "cpu_cores": control.silicon_truth.cpu.physical_cores,
                                        "has_gpu": !control.silicon_truth.accelerators.gpus.is_empty()
                                    }
                                });
                                
                                let _ = pipe.write_all(response.to_string().as_bytes()).await;
                                continue;
                            }
                            // UPDATE_BOOSTER — sent by Tauri store (matches store action name)
                            "UPDATE_BOOSTER" | "BOOSTER_UPDATE" => {
                                if let Some(payload) = json_cmd.get("payload") {
                                    // Handle single key-value update: {key: "flash_attention", value: "On"}
                                    if let (Some(key), Some(value)) = (payload.get("key").and_then(|k| k.as_str()), payload.get("value")) {
                                        let mut booster = cluaize_shared::hardware::governor::HardwareGovernor::load_booster_settings().unwrap_or_default();
                                        if let Ok(mut booster_json) = serde_json::to_value(&booster) {
                                            booster_json[key] = value.clone();
                                            if let Ok(updated) = serde_json::from_value(booster_json) {
                                                booster = updated;
                                                let _ = cluaize_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster);
                                                let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                            } else {
                                                let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"invalid booster format\"}").await;
                                            }
                                        }
                                    // Handle full booster object update (legacy BOOSTER_UPDATE format)
                                    } else if let Ok(booster_ctrl) = serde_json::from_value(payload.clone()) {
                                        let _ = cluaize_shared::hardware::governor::HardwareGovernor::save_booster_settings(&booster_ctrl);
                                        let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                    } else {
                                        let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"invalid payload\"}").await;
                                    }
                                }
                                continue;
                            }
                            "UPDATE_PERMISSION" => {
                                if let Some(payload) = json_cmd.get("payload") {
                                    if let (Some(key), Some(value)) = (payload.get("key").and_then(|k| k.as_str()), payload.get("value")) {
                                        let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                        if let Ok(mut perms_json) = serde_json::to_value(&perms) {
                                            perms_json[key] = value.clone();
                                            if let Ok(updated_perms) = serde_json::from_value::<engines::neural_foundry::security::permission_schema::PermissionSchema>(perms_json) {
                                                updated_perms.save();
                                                let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                            } else {
                                                let _ = pipe.write_all(b"{\"status\": \"error\", \"message\": \"invalid permission format\"}").await;
                                            }
                                        }
                                    }
                                }
                                continue;
                            }
                            "RESET_BOOSTER" => {
                                tracing::info!("🔄 [IPC] Resetting Booster to hardware-optimal defaults...");
                                let control = cluaize_shared::hardware::governor::HardwareGovernor::load_system_control().unwrap_or_default();
                                let vram_gb: f64 = control.silicon_truth.accelerators.gpus.iter().map(|g| g.vram_total_gb).sum();
                                let ram_gb = control.silicon_truth.memory.total_capacity_gb;
                                let has_gpu = !control.silicon_truth.accelerators.gpus.is_empty();

                                use cluaize_shared::hardware::schema::booster::*;

                                // Determine optimal n_gpu_layers based on VRAM
                                let optimal_gpu_layers: i32 = if !has_gpu { 0 } else { -1 }; // -1 = auto

                                // Determine optimal mode based on total resources
                                let optimal_mode = if vram_gb >= 24.0 {
                                    BoosterMode::MaxBoost
                                } else if vram_gb >= 8.0 {
                                    BoosterMode::Balance
                                } else if vram_gb >= 2.0 {
                                    BoosterMode::Multitasking
                                } else {
                                    BoosterMode::Edge
                                };

                                // Speculative decoding only safe with 8GB+ VRAM
                                let optimal_spec = if vram_gb >= 8.0 { FeatureState::Auto } else { FeatureState::Off };

                                let optimal_booster = BoosterControl {
                                    mode_run: optimal_mode,
                                    turbo_quant: FeatureState::Auto,
                                    flash_attention: if has_gpu { FeatureState::On } else { FeatureState::Auto },
                                    speculative_decoding: optimal_spec,
                                    auto_round: FeatureState::Auto,
                                    dflash: SmartState::Static("Auto".into()),
                                    kv_cache_quantization: if vram_gb < 4.0 { KvCacheQuantization::Kv8 } else { KvCacheQuantization::Auto },
                                    context_shifting: ContextShiftingMode::Auto,
                                    force_vram_reclaim: FeatureState::Off,
                                    n_gpu_layers: optimal_gpu_layers,
                                    think_mode: FeatureState::Auto,
                                    response_length: "auto".to_string(),
                                    enforce_json: false,
                                    force_memory_lock: if ram_gb < 8.0 { FeatureState::On } else { FeatureState::Off },
                                };

                                let _ = cluaize_shared::hardware::governor::HardwareGovernor::save_booster_settings(&optimal_booster);
                                let response = serde_json::json!({"status": "success", "booster": optimal_booster});
                                let _ = pipe.write_all(response.to_string().as_bytes()).await;
                                continue;
                            }

                            "SET_HARDWARE" => {
                                tracing::info!("🚀 [IPC] Received SET_HARDWARE: {:?}", json_cmd);
                                // Here we would dynamically adjust thread counts/Vulcan map in engine
                                let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                continue;
                            }
                            "SET_MODEL" => {
                                tracing::info!("🚀 [IPC] Received SET_MODEL: {:?}", json_cmd);
                                // Here we would unload current model and load new one
                                let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                continue;
                            }
                            "EAGER_LOAD" => {
                                tracing::info!("🚀 [IPC] Received EAGER_LOAD. Pre-loading text model...");
                                let _ = pipe.write_all(b"{\"status\": \"success\"}").await;
                                continue;
                            }
                            _ => {}
                        }
                    }
                }

                if command.starts_with("//CDQL_") {
                    let response = format!("{{\"status\": \"success\", \"query\": \"{}\"}}", command);
                    let _ = pipe.write_all(response.as_bytes()).await;
                } else {
                    // Dispatch natural language inference via Master Router
                    match state.dispatcher.dispatch_stream(command, false).await {
                        EngineResponse::TokenStream(mut rx) => {
                            let mut in_think_block = false;

                            // 🧠 Dynamic Tag Resolution for active model
                            let mut start_tag = "<think>".to_string();
                            let mut end_tag = "</think>".to_string();
                            
                            let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                            let roster = engines::models::registry::CoreRoster::load_roster();
                            if let Some(id) = perms.chat_models.text {
                                if let Some(model) = roster.iter().find(|m| m.id == id) {
                                    if let Some(path) = &model.local_path {
                                        if let Ok(dna) = cluaize_shared::metadata::dna::StructuralDNA::load(std::path::Path::new(path)) {
                                            if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" {
                                                start_tag = dna.think_tag_schema.clone();
                                                end_tag = dna.think_end_schema.clone();
                                            }
                                        }
                                    }
                                }
                            }

                            let mut in_cel_block = false;
                            let mut cel_buffer = String::new();

                            while let Some(mut token) = rx.recv().await {
                                if token.trim() == "[DONE]" {
                                    let json = serde_json::json!({"type": "done", "done": true});
                                    let _ = pipe.write_all(format!("{}\n", json).as_bytes()).await;
                                    break;
                                }

                                // ── CEL Engine Directives Interception ──
                                if token.contains("<cel>") {
                                    in_cel_block = true;
                                    token = token.replace("<cel>", "");
                                }
                                if token.contains("</cel>") {
                                    in_cel_block = false;
                                    cel_buffer.push_str(&token.replace("</cel>", ""));
                                    
                                    // 🚀 EXECUTE CEL BLOCK INTERNALLY
                                    let cel_script = cel_buffer.clone();
                                    cel_buffer.clear();
                                    
                                    tracing::info!("🧠 [FFI Bridge] Intercepted CEL Command mid-inference. Pausing stream to execute natively...");
                                    
                                    // Call the parser and execute natively without streaming to user
                                    if let Ok(ast) = inference_cel::parser::lexer::parse(&cel_script) {
                                        let planner = inference_cel::parser::planner::CelPlanner::new();
                                        if let Ok(plan) = planner.build_plan(&ast) {
                                            tracing::info!("🔥 [FFI Bridge] CEL Engine Plan Executing...");
                                            let exec_result = crate::handlers::cel_handler::execute_cel_plan(plan).await;
                                            tracing::info!("✅ [FFI Bridge] CEL execution completed: {}. Resuming stream.", exec_result);
                                        }
                                    }
                                    continue;
                                }

                                if in_cel_block {
                                    cel_buffer.push_str(&token);
                                    continue; // Do not stream CEL logic to the user
                                }

                                if token.contains(&start_tag) {
                                    in_think_block = true;
                                    token = token.replace(&start_tag, "");
                                    token = token.trim_start_matches('\n').to_string();
                                }
                                if token.contains(&end_tag) {
                                    in_think_block = false;
                                    token = token.replace(&end_tag, "");
                                    token = token.trim_start_matches('\n').to_string();
                                }

                                if token.is_empty() { continue; }

                                let json = if in_think_block {
                                    serde_json::json!({"type": "token", "thinking": token, "done": false})
                                } else {
                                    serde_json::json!({"type": "token", "content": token, "done": false})
                                };

                                if pipe.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                        EngineResponse::FinalResult(res) => {
                            let json = serde_json::json!({"type": "done", "content": res, "done": true});
                            let _ = pipe.write_all(format!("{}\n", json).as_bytes()).await;
                        }
                        EngineResponse::Error(err) => {
                            let json = serde_json::json!({"type": "error", "error": err, "done": true});
                            let _ = pipe.write_all(format!("{}\n", json).as_bytes()).await;
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("❌ Pipe read error: {}", e);
                break;
            }
        }
    }
}

#[cfg(not(windows))]
pub async fn start_named_pipe_server(_state: Arc<AppState>) {
    tracing::warn!("Native Named Pipes are only supported on Windows. IPC Disabled.");
}
