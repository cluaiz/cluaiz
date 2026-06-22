use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;

/// `cluaize run <model-id>` â€” pulls the model and initiates a native chat session.
pub async fn execute(model_id: &str, _interactive: bool) -> Result<()> {
    // ðŸŽ¨ Display the Sovereign Logo
    let logo = crate::assets::logos::logo_gallery::LOGO_VARIANTS[9];
    println!("{}", logo.cyan());

    println!("\n  {} [Cluaize] Initializing Kernel for '{}'...", "âš™ï¸".yellow(), model_id.bold());

    let mut manifest: Option<engines::models::registry::ModelManifest> = None;
    let mut is_local = false;
    let mut is_hf = false;
    let mut resolved_id = model_id.to_string();

    let roster = CoreRoster::load_roster();
    let cluaize_root = cluaize_shared::environment::EnvironmentManager::current().models_dir();

    if model_id.contains('/') {
        // ðŸš€ EXPLICIT HUGGINGFACE REQUEST
        is_hf = true;
        resolved_id = model_id.to_string();
        if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") {
            resolved_id = format!("hf://{}", resolved_id);
        }
        
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "ðŸ”".cyan(), repo_id);
        
        let variants = engines::models::manager::hf_hub::HuggingFaceHub::list_variants(&repo_id).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.filename, v.size_gb)).collect();
        let selection = inquire::Select::new("Select model variant to download:", options).prompt()
            .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?;
            
        let selected_filename = selection.split(" (").next().unwrap().to_string();
        
        // ðŸš€ NEW: Check if this specific variant is already in the local roster!
        if let Some(existing) = roster.iter().find(|m| m.huggingface_repo.to_lowercase() == repo_id.to_lowercase() && m.huggingface_filename.to_lowercase() == selected_filename.to_lowercase()) {
            println!("\n  {} Warning: This exact variant is already downloaded locally under ID: '{}'", "âš ï¸".yellow(), existing.id.cyan());
            println!("     To run it instantly, use: cluaize run {}", existing.id.green());
            println!("     If you wish to re-download, please delete the old one first using: cluaize rm {}\n", existing.id.red());
            return Ok(());
        }

        let selected_size_str = selection.split(" (").nth(1).unwrap().replace(" GB)", "");
        let selected_size_gb: f64 = selected_size_str.parse().unwrap_or(0.0);
        
        println!("  {} Fetching precise metadata...", "ðŸ“¡".cyan());
        let hf_manifest = engines::models::manager::hf_hub::HuggingFaceHub::build_manifest(&repo_id, &selected_filename, selected_size_gb).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        manifest = Some(hf_manifest);
        is_local = false;
    } else {
        // ðŸš€ REGISTRY OR LOCAL ID REQUEST
        if let Some(m) = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase()) {
            let safe_id = m.id.replace(':', "-");
            let model_path = cluaize_root.join(&m.category).join(&safe_id);
            let model_file = model_path.join(&m.huggingface_filename);

            if model_file.exists() {
                manifest = Some(m);
                is_local = true;
            } else {
                manifest = Some(m);
                is_local = false;
            }
        } else {
            // Not in local vault, fetch from external registry
            println!("  {} Model missing in local vault. Synchronizing with Neural Registry...", "ðŸŒ".yellow());
            let remote_models = CoreRoster::fetch_external_registry(None).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }
    }

    let mut manifest = manifest.ok_or_else(|| color_eyre::eyre::eyre!("ID '{}' not found in any registry.", model_id))?;

    // ðŸš€ Update the Engine Permission.json with the actively running model so CompilerDaemon knows what to compile
    if manifest.architecture_type == "onnx" {
        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(manifest.id.clone());
    } else {
        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(manifest.id.clone());
    }

    // ðŸš€ Trigger Skill Registry (which triggers CompilerDaemon) to provision the caches for this active model
    let skills_dir = cluaize_shared::environment::EnvironmentManager::current().skills_dir();
    if skills_dir.exists() {
        let mut registry = engines::neural_foundry::registry::SkillRegistry::new();
        registry.load_from_directory(&skills_dir.to_string_lossy());
    }

    // 2. Silicon Audit (Local Probe or HF Metadata)
    let manager = engines::models::manager::ModelManager::new(engines::models::registry::REGISTRY_URL.to_string(), cluaize_root.clone());
    
    let safe_id = manifest.id.replace(':', "-");
    let model_path = cluaize_root.join(&manifest.category).join(&safe_id);
    let model_file = model_path.join(&manifest.huggingface_filename);

    if !is_local {
        println!("  {} Fetching Deep Metadata (Binary Probe)...", "ðŸ“¡".cyan());
    }
    
    let is_onnx = manifest.architecture_type == "onnx";
    
    if is_onnx {
        // ONNX specific metadata display
        if !is_local {
            println!("    â”œâ”€ ðŸ§  Architecture: {}", manifest.architecture.yellow());
            println!("    â”œâ”€ ðŸ§© Type: ONNX Optimized Execution Graph");
            println!("    â”œâ”€ ðŸ“¦ Quantization: fp32");
            println!("    â”œâ”€ ðŸ’¾ Download Size: {:.2} GB", manifest.download_size_gb);
            println!("    â”œâ”€ âš™ï¸ RAM Requirement: {:.2} GB", manifest.ram_required_gb);
        }
    } else {
        let probe_result = if is_local && model_file.exists() {
            cluaize_shared::utils::gguf_prober::GGUFProber::probe(&model_file).map_err(|e| e.to_string())
        } else {
            engines::models::manager::hf_hub::HuggingFaceHub::fetch_partial_gguf_metadata(&manifest.download_url).await
        };

        if let Ok((metadata, _tensor_infos, tensor_count)) = probe_result {
            let arch = metadata.get("general.architecture").unwrap_or(&"Unknown".to_string()).to_string();
            let ctx = metadata.get(&format!("{}.context_length", arch)).or(metadata.get("llama.context_length")).unwrap_or(&"Unknown".to_string()).to_string();
            
            let ctx_display = if let Ok(ctx_val) = ctx.parse::<u32>() {
                if ctx_val >= 1024 {
                    format!("{} ({}K)", ctx_val, ctx_val / 1024)
                } else {
                    ctx.clone()
                }
            } else {
                ctx.clone()
            };

            let params = metadata.get("general.parameter_count").unwrap_or(&"Unknown".to_string()).to_string();
            let file_type = metadata.get("general.file_type").unwrap_or(&"Unknown".to_string()).to_string();
            let blocks = metadata.get(&format!("{}.block_count", arch)).unwrap_or(&"Unknown".to_string()).to_string();
            
            let num_layers = blocks.parse::<u64>().unwrap_or(32);
            let num_heads = metadata.get(&format!("{}.attention.head_count", arch)).and_then(|s| s.parse::<u64>().ok()).unwrap_or(32);
            let num_kv_heads = metadata.get(&format!("{}.attention.head_count_kv", arch)).and_then(|s| s.parse::<u64>().ok()).unwrap_or(num_heads);
            let hidden_size = metadata.get(&format!("{}.embedding_length", arch)).and_then(|s| s.parse::<u64>().ok()).unwrap_or(4096);
            
            let head_dim = hidden_size / num_heads.max(1);
            let standard_context_tokens = 8192;
            let kv_cache_bytes = 2 * 2 * num_layers * num_kv_heads * head_dim * standard_context_tokens;
            let kv_cache_gb = kv_cache_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let base_engine_overhead_gb = 0.30;
            
            // Dynamically override manifest RAM based on exact architectural math
            manifest.ram_required_gb = manifest.download_size_gb + base_engine_overhead_gb + kv_cache_gb;

            if !is_local {
                println!("    â”œâ”€ ðŸ§  Architecture: {}", arch.yellow());
                println!("    â”œâ”€ ðŸ“ Context Window: {} tokens", ctx_display.green());
                println!("    â”œâ”€ ðŸ§© Parameters: {} B", params.green());
                println!("    â”œâ”€ ðŸ“¦ Quantization / File Type: {}", file_type.cyan());
                println!("    â”œâ”€ ðŸ“š Network Layers (Blocks): {}", blocks.magenta());
                println!("    â”œâ”€ âš¡ Tensor Count: {}", tensor_count.to_string().cyan());
                println!("    â”œâ”€ ðŸ’¾ Download Size: {:.2} GB", manifest.download_size_gb);
                println!("    â”œâ”€ ðŸ§® KV Cache (8K tokens): {:.2} GB", kv_cache_gb);
                println!("    â”œâ”€ âš™ï¸ Base Engine Overhead: {:.2} GB", base_engine_overhead_gb);
            }
        } else if let Err(e) = probe_result {
            if !is_local {
                println!("    â”œâ”€ âš ï¸ Could not probe remote GGUF header: {}", e);
                println!("    â”œâ”€ ðŸ’¾ Download Size: {:.2} GB", manifest.download_size_gb);
            }
        }
    }
    
    // Load System Control to get real hardware stats
    let config_path = cluaize_shared::hardware::governor::HardwareGovernor::resolve_engine_path().join("system_control.json");
    let system_control = if let Ok(content) = std::fs::read_to_string(config_path) {
        serde_json::from_str::<cluaize_shared::hardware::schema::profiles::SystemControl>(&content).ok()
    } else {
        None
    };

    let (mut user_ram, mut user_vram) = (0.0, 0.0);
    if let Some(control) = &system_control {
        user_ram = control.silicon_truth.memory.total_capacity_gb;
        user_vram = control.silicon_truth.accelerators.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
    }
    
    let total_required = manifest.ram_required_gb;
    let mut projected_tps = 0.0;
    
    if user_vram > 0.0 {
        if total_required <= user_vram {
            projected_tps = 35.0; 
        } else {
            let vram_ratio = user_vram / total_required;
            if vram_ratio > 0.8 { projected_tps = 22.0; }
            else if vram_ratio > 0.5 { projected_tps = 15.0; }
            else { projected_tps = 8.0; }
        }
    } else {
        projected_tps = 5.0;
    }

    let status = manager.audit_model_health(total_required as f32, manifest.requires_gpu);

    if !is_local {
        println!("\n  {} Conducting Pre-flight Silicon Audit...", "âš–ï¸".cyan());
        println!("    â”œâ”€ ðŸ–¥ï¸ Host System RAM: {:.2} GB", user_ram);
        if user_vram > 0.0 {
            println!("    â”œâ”€ ðŸŽ® Target VRAM (Primary GPU): {:.2} GB", user_vram);
        }
        println!("    â”œâ”€ ðŸ“Š Target Allocation: {:.2} GB (Weights + Engine + 8K Context)", total_required);
        
        if user_vram > 0.0 {
            if total_required <= user_vram {
                println!("    â”œâ”€ âš¡ Offload Status: Full GPU Acceleration (100% VRAM)");
                println!("    â”œâ”€ ðŸ§® Remaining VRAM post-load: {:.2} GB", user_vram - total_required);
            } else {
                let vram_ratio = user_vram / total_required;
                println!("    â”œâ”€ âš¡ Offload Status: Partial GPU Acceleration ({:.0}% in VRAM)", vram_ratio * 100.0);
                println!("    â”œâ”€ ðŸ§® Remaining System RAM post-load: {:.2} GB", user_ram - (total_required - user_vram));
            }
        } else {
            println!("    â”œâ”€ âš¡ Offload Status: CPU Inference (No dedicated VRAM)");
            println!("    â”œâ”€ ðŸ§® Remaining System RAM post-load: {:.2} GB", user_ram - total_required);
        }
        println!("    â”œâ”€ ðŸš€ Projected Speed: ~{:.0} Tokens/Second (TPS)", projected_tps);
        println!("    â”œâ”€ System Status: {:?}", status);
    }
    
    if status == engines::models::manager::auditor::HealthStatus::Disabled {
        return Err(color_eyre::eyre::eyre!("âŒ DENIED: Insufficient hardware resources for this model."));
    }
    
    if !is_local {
        if is_hf {
            let confirm = inquire::Confirm::new("Audit passed. Proceed with model download?").with_default(true).prompt()?;
            if !confirm {
                return Err(color_eyre::eyre::eyre!("Initialization aborted by user."));
            }
            manager.pull_model_with_manifest(&manifest).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            
            println!("\n  {} HuggingFace Model Downloaded! Launching dynamic session...", "âœ…".green());
            is_local = true;
        } else {
            let confirm = inquire::Confirm::new("Audit passed. Proceed with model download and initialization?").with_default(true).prompt()?;
            if !confirm {
                return Err(color_eyre::eyre::eyre!("Initialization aborted by user."));
            }
            manager.pull_model(&resolved_id).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            is_local = true;
        }
    }

    if !model_file.exists() {
        return Err(color_eyre::eyre::eyre!("Model file not found at: {:?}", model_file));
    }

    if is_local {
        println!("  {} Local Audit Passed. Preparing Neural Matrix...", "âœ¨".green());
    }

    // Give a small pause for visual feedback before clearing screen for dashboard
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 4. Launch Dashboard in Sovereign Mode (Pre-loaded with the model)
    use crate::core::state::AppState;
    use tokio::sync::mpsc;
    
    // ðŸ§¬ Load Real Tokenizer from the model folder
    let repo_id = if manifest.download_url.contains("huggingface.co/") {
        manifest.download_url
            .split("huggingface.co/")
            .nth(1)
            .unwrap_or("")
            .split("/resolve")
            .next()
            .unwrap_or(&manifest.id)
            .to_string()
    } else {
        manifest.id.clone()
    };
    let _ = engines::utils::healer::AutoHealer::heal_missing_tokenizer(&repo_id, &model_path).await;
    let tokenizer_path = model_path.join("tokenizer.json");
    let mut state = AppState::new(None, None);
    
    state.Core_engine.load_model(model_file.clone()).await
        .map_err(|e| color_eyre::eyre::eyre!("Model loading failed: {}", e))?;
    
    state._active_model_id = Some(manifest.id.clone());

    if _interactive {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mode = crate::app_enums::Mode::Running;

        // ðŸš€ Start the Dashboard UI
        crate::core::dashboard::DashboardEngine::run_native(
            &mut state,
            &tx,
            &mut rx,
            &mut mode
        )?;
    } else {
        println!("\nâœ¨ Non-interactive Batch Mode Active.");
        // Read prompts line-by-line from stdin
        use std::io::{BufRead, Write};
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        loop {
            print!("? > ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            if handle.read_line(&mut line)? == 0 {
                break; // EOF
            }
            let prompt = line.trim();
            if prompt.is_empty() {
                continue;
            }
            if prompt == "exit" || prompt == "quit" {
                break;
            }
            
            println!("ðŸ¤– ");
            let accumulated_output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let output_clone = accumulated_output.clone();
            let res = tokio::task::block_in_place(|| -> Result<(), color_eyre::eyre::Report> {
                // â”€â”€ Native IPC Named Pipe Client â”€â”€
                let pipe_name = r"\\.\pipe\cluaize_engine_pipe";
                let mut client = match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(pipe_name) {
                    Ok(client) => client,
                    Err(e) => {
                        return Err(color_eyre::eyre::eyre!("âŒ Failed to connect to Native Daemon IPC (Is cluaizd running?): {}", e));
                    }
                };
                
                use std::io::{Read, Write};
                // Send the prompt natively to the daemon
                if let Err(e) = client.write_all(prompt.as_bytes()) {
                     return Err(color_eyre::eyre::eyre!("âŒ Failed to send command to IPC: {}", e));
                }
                
                // Read streaming tokens with 0ms latency
                let mut buffer = [0; 1024];
                loop {
                    match client.read(&mut buffer) {
                        Ok(0) => break, // Pipe closed
                        Ok(n) => {
                            let token = String::from_utf8_lossy(&buffer[..n]);
                            if token.trim() == "[DONE]" { break; }
                            print!("{}", token);
                            let _ = std::io::stdout().flush();
                            if let Ok(mut guard) = output_clone.lock() {
                                guard.push_str(&token);
                            }
                        }
                        Err(e) => {
                             return Err(color_eyre::eyre::eyre!("âŒ IPC Read Error: {}", e));
                        }
                    }
                }
                Ok(())
            });
            if let Err(e) = res {
                println!("\nâŒ Inference Error: {}", e);
            } else {
                let clean_output = accumulated_output.lock().unwrap().trim().to_string();
                let mut json_str = None;
                if let Some(start) = clean_output.find('{') {
                    if let Some(end) = clean_output.rfind('}') {
                        if end > start {
                            json_str = Some(clean_output[start..=end].to_string());
                        }
                    }
                }
                if let Some(js) = json_str {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&js) {
                        if let Some(action) = val.get("action").and_then(|v| v.as_str()) {
                            println!("\nâš™ï¸ [CLI REPL] Intercepted JSON ABI tool call: {}", action.bold().cyan());
                            
                            let mut skill_manifest = None;
                            let mut logic_path = None;
                            let mut is_allowed = false;
                            
                            {
                                let router = state.Core_engine.router.lock().await;
                                if let Some(skill) = router.foundry.registry.skills.iter().find(|s| &s.manifest.id == action) {
                                    skill_manifest = Some(skill.manifest.clone());
                                    logic_path = Some(skill.path.join("logic.wasm"));
                                    is_allowed = router.foundry.guard.validate_action(&skill.manifest, engines::neural_foundry::security::guard::PermissionLevel::ReadOnly).is_ok();
                                }
                            }
                            
                            if let Some(manifest) = skill_manifest {
                                let l_path = logic_path.unwrap();
                                if is_allowed && l_path.exists() {
                                    println!("âš™ï¸ [CLI REPL] Executing WASM Sandbox for: {}", manifest.name.green());
                                    let mut router = state.Core_engine.router.lock().await;
                                    let wasm_res = router.foundry.wasm_runtime.execute_skill_logic(&l_path, "run", prompt).await;
                                    match wasm_res {
                                        Ok(output) => {
                                            println!("\nðŸ’» [WASM Sandbox Output]:");
                                            println!("{}", output.green());
                                        }
                                        Err(e) => {
                                            println!("\nâŒ [WASM Sandbox Execution Failed]: {}", e);
                                        }
                                    }
                                } else if !l_path.exists() {
                                    println!("âš ï¸ [CLI REPL] logic.wasm not found for skill: {}", action);
                                }
                            } else {
                                println!("âš ï¸ [CLI REPL] Skill not found in registry: {}", action);
                            }
                        }
                    }
                }
            }
            println!("\n");
        }
    }

    Ok(())
}
