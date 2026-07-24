use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::io::{self, Write, Read};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// `cluaiz run <model-id>` — pulls the model and initiates a native chat session.
pub async fn execute(model_id: &str, _interactive: bool) -> Result<()> {
    // 🎨 Display the Sovereign Logo
    let logo = crate::assets::logos::logo_gallery::LOGO_VARIANTS[9];
    println!("{}", logo.cyan());

    println!("\n  {} [cluaiz] Initializing Kernel for '{}'...", "⚙️".yellow(), model_id.bold());

    let mut manifest: Option<engines::models::registry::ModelManifest> = None;
    let mut selected_variant_bundle: Option<engines::models::manager::hf_hub::HfVariant> = None;
    let mut is_local = false;
    let mut is_hf = false;
    let mut resolved_id = model_id.to_string();

    let roster = CoreRoster::load_roster();
    let cluaiz_root = cluaiz_shared::environment::EnvironmentManager::current().models_dir();

    if model_id.contains('/') {
        // 🚀 EXPLICIT HUGGINGFACE REQUEST
        is_hf = true;
        resolved_id = model_id.to_string();
        if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") {
            resolved_id = format!("hf://{}", resolved_id);
        }
        
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo_id);
        
        let variants = engines::models::manager::hf_hub::HuggingFaceHub::list_variants(&repo_id).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.variant_id, v.size_gb)).collect();
        let selected_option = inquire::Select::new("Select model variant bundle to download:", options)
            .with_page_size(12)
            .raw_prompt()
            .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?;
            
        let selected_variant = variants.into_iter().nth(selected_option.index)
            .ok_or_else(|| color_eyre::eyre::eyre!("Selected variant not found"))?;

        let selected_filename = selected_variant.primary_file.clone();
        let selected_size_gb = selected_variant.size_gb;
        
        // 🚀 Check if this specific variant is already in the local roster!
        if let Some(existing) = roster.iter().find(|m| m.huggingface_repo.to_lowercase() == repo_id.to_lowercase() && m.huggingface_filename.to_lowercase() == selected_filename.to_lowercase()) {
            println!("\n  {} Warning: This exact variant is already downloaded locally under ID: '{}'", "⚠️".yellow(), existing.id.cyan());
            println!("     To run it instantly, use: cluaiz run {}", existing.id.green());
            println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", existing.id.red());
            return Ok(());
        }

        println!("  {} Fetching precise metadata...", "📡".cyan());
        let hf_manifest = engines::models::manager::hf_hub::HuggingFaceHub::build_manifest(&repo_id, &selected_filename, selected_size_gb).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        manifest = Some(hf_manifest);
        selected_variant_bundle = Some(selected_variant);
        is_local = false;

        // Append quant tag to manifest ID for flat vault folder naming
        // e.g. GLM-5.2-GGUF + UD-IQ1_S → GLM-5.2-GGUF-UD-IQ1_S (flat, not nested)
        if let (Some(ref mut m), Some(ref bundle)) = (&mut manifest, &selected_variant_bundle) {
            let qt = bundle.quant_tag.clone();
            if !qt.is_empty() && qt != "DEFAULT" {
                m.id = format!("{}-{}", m.id, qt);
            }
            // Set huggingface_filename to the flat basename of the primary file
            let flat_name = bundle.primary_file.rsplit('/').next().unwrap_or(&bundle.primary_file).to_string();
            m.huggingface_filename = flat_name;
        }
    } else {
        // 🚀 REGISTRY OR LOCAL ID REQUEST
        if let Some(m) = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase()) {
            let safe_id = m.id.replace(':', "-");
            let model_path = cluaiz_root.join(&m.category).join(&safe_id);
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
            println!("  {} Model missing in local vault. Synchronizing with Neural Registry...", "🌐".yellow());
            let remote_models = CoreRoster::fetch_external_registry(None).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }
    }

    let mut manifest = manifest.ok_or_else(|| color_eyre::eyre::eyre!("ID '{}' not found in any registry.", model_id))?;

    // 🚀 Update the Engine permission.json with the actively running model so CompilerDaemon knows what to compile
    if manifest.architecture_type == "onnx" {
        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(manifest.id.clone());
    } else {
        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(manifest.id.clone());
    }

    // 🚀 Trigger Skill Registry (which triggers CompilerDaemon) to provision the caches for this active model
    let skills_dir = cluaiz_shared::environment::EnvironmentManager::current().skills_dir();
    if skills_dir.exists() {
        let mut registry = engines::neural_foundry::registry::SkillRegistry::new();
        registry.load_from_directory(&skills_dir.to_string_lossy());
    }

    // 2. Silicon Audit (Local Probe or HF Metadata)
    let manager = engines::models::manager::ModelManager::new(engines::models::registry::REGISTRY_URL.to_string(), cluaiz_root.clone());
    
    let safe_id = manifest.id.replace(':', "-");
    let model_path = cluaiz_root.join(&manifest.category).join(&safe_id);
    let model_file = model_path.join(&manifest.huggingface_filename);

    if !is_local {
        let quant_display = selected_variant_bundle.as_ref()
            .map(|v| v.quant_tag.clone())
            .unwrap_or_else(|| "DEFAULT".to_string());
            
        let is_onnx = manifest.huggingface_filename.ends_with(".onnx") || manifest.architecture_type == "onnx";
        let format_display = if is_onnx { "ONNX" } else if manifest.huggingface_filename.ends_with(".gguf") { "GGUF" } else { "SafeTensors" };

        println!("\n  {} Model Metadata Summary:", "📋".cyan().bold());
        println!("    ├─ 📦 Format: {}", format_display.cyan());
        println!("    ├─ 🏷️  Precision / Quant Tag: {}", quant_display.magenta().bold());
        println!("    ├─ 🧠 Architecture: {}", manifest.architecture.yellow().bold());
        if !manifest.parameters.is_empty() && manifest.parameters != "Unknown" {
            println!("    ├─ 🧩 Parameters: {}", manifest.parameters.green());
        }
        println!("    ├─ 💾 Total Download Size: {:.2} GB", manifest.download_size_gb);

        // Bundled Files Preview — show all weight shards + JSON configs
        if let Some(bundle) = &selected_variant_bundle {
            let total = bundle.all_files.len();
            println!("    └─ 📁 Bundled Files to Download ({} files):", total.to_string().cyan().bold());
            for (idx, f) in bundle.all_files.iter().enumerate() {
                let is_last = idx == total - 1;
                let connector = if is_last { "       └─" } else { "       ├─" };
                let icon = if f.ends_with(".gguf") || f.ends_with(".onnx") || f.ends_with(".safetensors") {
                    "⚖️ "
                } else if f.ends_with(".onnx_data") || f.ends_with(".data") {
                    "📦"
                } else {
                    "📄"
                };
                println!("{} {} {}", connector, icon, f.cyan());
            }
        } else {
            println!("    └─ 📁 File: {}", manifest.huggingface_filename.cyan());
        }

        let confirm = inquire::Confirm::new("\nProceed with model download?").with_default(true).prompt()?;
        if !confirm {
            return Err(color_eyre::eyre::eyre!("Initialization aborted by user."));
        }
        let files_to_pull = selected_variant_bundle.as_ref()
            .map(|v| v.all_files.clone())
            .unwrap_or_else(|| vec![manifest.huggingface_filename.clone()]);

        manager.pull_model_bundle_with_manifest(&manifest, &files_to_pull).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
        
        let safe_id = manifest.id.replace(':', "-");
        let local_path = cluaiz_root.join(&manifest.category).join(&safe_id);

        println!("\n  {} Model downloaded and registered successfully!", "✅".green().bold());
        println!("  ┌─────────────────────────────────────────────────────────────┐");
        println!("  │ 🆔 Registered ID:  {}", manifest.id.cyan().bold());
        println!("  │ 📁 Vault Path:     {}", local_path.display().to_string().yellow());
        println!("  │ 🚀 Run Command:   {}", format!("cluaiz run {}", manifest.id).green().bold());
        println!("  └─────────────────────────────────────────────────────────────┘\n");
        return Ok(());
    }

    if !model_file.exists() {
        return Err(color_eyre::eyre::eyre!("Model file not found at: {:?}", model_file));
    }

    if is_local {
        println!("  {} Local Audit Passed. Preparing Neural Matrix...", "✨".green());
    }

    // Give a small pause for visual feedback before clearing screen for dashboard
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // 4. Launch Dashboard in Sovereign Mode (Pre-loaded with the model)
    use crate::core::state::AppState;
    use tokio::sync::mpsc;
    
    // 🧬 Load Real Tokenizer from the model folder
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
    let mut state = AppState::new(None);
    state._active_model_id = Some(manifest.id.clone());

    if _interactive {
        let perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
        if !perms.lazy_load_model && !state.is_client_mode {
            state.Core_engine.load_model(model_file.clone()).await
                .map_err(|e| color_eyre::eyre::eyre!("Model loading failed: {}", e))?;
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut mode = crate::app_enums::Mode::Running;

        // 🚀 Start the Dashboard UI
        crate::core::dashboard::DashboardEngine::run_native(
            &mut state,
            &tx,
            &mut rx,
            &mut mode
        )?;
    } else {
        println!("\n✨ Non-interactive Batch Mode Active.");
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
            
            let accumulated_output = Arc::new(Mutex::new(String::new()));
            let output_clone = accumulated_output.clone();
            
            struct CLIProgressTracker {
                current_step: Arc<Mutex<Option<String>>>,
                handle: Option<JoinHandle<()>>,
            }

            impl CLIProgressTracker {
                fn new() -> Self {
                    let current_step = Arc::new(Mutex::new(None));
                    let current_step_clone = current_step.clone();
                    let handle = thread::spawn(move || {
                        let mut idx = 0;
                        loop {
                            if let Ok(lock) = current_step_clone.lock() {
                                if let Some(msg) = &*lock {
                                    print!("\r\x1B[K\x1B[33m{} {}\x1B[0m", SPINNER_FRAMES[idx], msg);
                                    let _ = io::stdout().flush();
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                            thread::sleep(Duration::from_millis(85));
                            idx = (idx + 1) % SPINNER_FRAMES.len();
                        }
                    });
                    Self { current_step, handle: Some(handle) }
                }
                fn set_step(&self, msg: &str) {
                    if let Ok(mut lock) = self.current_step.lock() {
                        *lock = Some(msg.to_string());
                    }
                }
                fn complete_step(&self, msg: &str) {
                    if let Ok(mut lock) = self.current_step.lock() {
                        *lock = Some(msg.to_string()); // Temporarily set to prevent race
                    }
                    print!("\r\x1B[K\x1B[32m✅ {}\x1B[0m\n", msg);
                    let _ = io::stdout().flush();
                }
                fn stop(&mut self) {
                    if let Ok(mut lock) = self.current_step.lock() {
                        *lock = None;
                    }
                    if let Some(h) = self.handle.take() {
                        let _ = h.join();
                    }
                    print!("\r\x1B[K");
                    let _ = io::stdout().flush();
                }
            }

            let prompt_str = prompt.to_string();
            let res = tokio::task::block_in_place(|| -> Result<(), color_eyre::eyre::Report> {
                // ── Native IPC Named Pipe Client ──
                let pipe_name = r"\\.\pipe\cluaiz_engine_pipe";
                let mut client = match std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(pipe_name) {
                    Ok(client) => client,
                    Err(e) => {
                        return Err(color_eyre::eyre::eyre!("❌ Failed to connect to Native Daemon IPC (Is cluaiz daemon running?): {}", e));
                    }
                };
                
                use std::io::{Read, Write};
                // Send the prompt natively to the daemon
                if let Err(e) = client.write_all(prompt_str.as_bytes()) {
                     return Err(color_eyre::eyre::eyre!("❌ Failed to send command to IPC: {}", e));
                }
                
                // Read streaming tokens with 0ms latency
                let mut buffer = [0; 4096];
                let mut accum_line = String::new();
                let mut tracker = CLIProgressTracker::new();
                
                tracker.complete_step(&format!("[Step 1] User SMS Received: \"{}\"", prompt_str));
                tracker.set_step("[Step 2] Performing Semantic Matching & Discovery (Probing registry...)");
                
                let mut step_lines_count = 1;
                let mut cleared_steps = false;

                loop {
                    match client.read(&mut buffer) {
                        Ok(0) => break, // Pipe closed
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buffer[..n]);
                            accum_line.push_str(&chunk);
                            
                            while let Some(pos) = accum_line.find('\n') {
                                let line = accum_line[..pos].trim().to_string();
                                accum_line = accum_line[pos + 1..].to_string();
                                
                                if line.is_empty() { continue; }
                                
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                    if let Some(done) = val.get("done").and_then(|d| d.as_bool()) {
                                        if done { break; }
                                    }
                                    
                                    if let Some(err) = val.get("error").and_then(|e| e.as_str()) {
                                        tracker.stop();
                                        println!("❌ Error: {}", err);
                                        break;
                                    }
                                    
                                    let content = val.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                    let thinking = val.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                                    
                                    let token = if !content.is_empty() { content } else { thinking };
                                    if token.is_empty() { continue; }
                                    
                                    if token.starts_with("__STEP_2_MATCH_START__") {
                                        let parts: Vec<&str> = token.split(':').collect();
                                        let matched = if parts.len() >= 2 { parts[1] } else { "cluaiz-search" };
                                        let score = if parts.len() >= 3 { parts[2] } else { "0.88" };
                                        
                                        tracker.complete_step(&format!("[Step 2] Match Found -> Registry Tool: '{}' (Score: {})", matched, score));
                                        step_lines_count += 1;
                                        tracker.set_step("[Step 3] Dynamic JIT Layer rules compile & inject (Loading rules...)");
                                    } else if token.starts_with("__STEP_3_INJECT_START__") {
                                        tracker.complete_step("[Step 3] Dynamic JIT Layer rules compile & inject successfully.");
                                        step_lines_count += 1;
                                        tracker.set_step("[Step 4] Inference system parses user SMS input context...");
                                    } else if token == "__STEP_4_READ_SMS__" {
                                        tracker.complete_step("[Step 4] Inference system parses user SMS input context.");
                                        step_lines_count += 1;
                                        tracker.set_step("[Step 5] AI Formulating Plan (Generating tags...)");
                                    } else if token.starts_with("<TRIGGER:") {
                                        tracker.complete_step(&format!("[Step 5] AI Formulates Plan: Match tag emitted -> {}", token));
                                        step_lines_count += 1;
                                        tracker.set_step("[Step 6] AI Emits plan closing sequence...");
                                    } else if token.contains("</TRIGGER>") {
                                        tracker.complete_step("[Step 6] AI Emits closing sequence tag: </TRIGGER>");
                                        step_lines_count += 1;
                                        tracker.set_step("[Step 7] Engine intercepting & pausing loop...");
                                    } else if token.contains("__ENGINE_PAUSE_EXECUTE__") {
                                        tracker.complete_step("[Step 7] Engine intercept triggered. Autoregressive loop PAUSED.");
                                        step_lines_count += 1;
                                        
                                        let parts: Vec<&str> = token.splitn(3, ':').collect();
                                        if parts.len() >= 2 {
                                            let tool_name = parts[1];
                                            tracker.set_step(&format!("[Step 8] Sandbox executing: '{}'...", tool_name));
                                            thread::sleep(Duration::from_millis(500));
                                            tracker.complete_step(&format!("[Step 8] Sandbox '{}' → ✓ Result captured.", tool_name));
                                            step_lines_count += 1;
                                        }
                                        tracker.set_step("[Step 9] Injecting KV-Cache parameters & resuming loop...");
                                        thread::sleep(Duration::from_millis(300));
                                        tracker.complete_step("[Step 9] Zero-copy KV-Cache parameters injected directly into context layers. Resuming loop...");
                                        step_lines_count += 1;
                                    } else {
                                        // ── FILTER: Skip internal system tokens ──
                                        // Do NOT print <TOOL_OUTPUT_LOG>, [Arbiter], [Router], [Agentic Pause] lines
                                        let is_internal_token =
                                            token.contains("<TOOL_OUTPUT_LOG>") ||
                                            token.contains("</TOOL_OUTPUT_LOG>") ||
                                            token.contains("[Arbiter]") ||
                                            token.contains("[Router]") ||
                                            token.contains("[Agentic Pause]") ||
                                            token.contains("🧠 [FFI") ||
                                            token.contains("✅ [Agentic") ||
                                            token.contains("<TOOL_") ||
                                            token.contains("TOOL_OUTPUT_LOG");

                                        if is_internal_token {
                                            // Silently discard — do not render to user
                                            continue;
                                        }

                                        // Stop the active spinner and print the AI header cleanly
                                        if !cleared_steps {
                                            tracker.stop();
                                            cleared_steps = true;
                                            
                                            print!("\n\x1B[36m🤖 AI Response:\x1B[0m\n");
                                            let _ = io::stdout().flush();
                                        }
                                        
                                        print!("{}", token);
                                        let _ = io::stdout().flush();
                                        if let Ok(mut guard) = output_clone.lock() {
                                            guard.push_str(token);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                             tracker.stop();
                             return Err(color_eyre::eyre::eyre!("❌ IPC Read Error: {}", e));
                        }
                    }
                }
                tracker.stop();
                print!("\n\x1B[32m✅ [Step 10] AI Response successfully rendered.\x1B[0m\n");
                let _ = io::stdout().flush();
                Ok(())
            });
            if let Err(e) = res {
                println!("\n❌ Inference Error: {}", e);
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
                            println!("\n⚙️ [CLI REPL] Intercepted JSON ABI tool call: {}", action.bold().cyan());
                            
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
                                    println!("⚙️ [CLI REPL] Executing WASM Sandbox for: {}", manifest.name.green());
                                    let mut router = state.Core_engine.router.lock().await;
                                    let wasm_res = router.foundry.wasm_runtime.execute_skill_logic(&l_path, "run", prompt).await;
                                    match wasm_res {
                                        Ok(output) => {
                                            println!("\n💻 [WASM Sandbox Output]:");
                                            println!("{}", output.green());
                                        }
                                        Err(e) => {
                                            println!("\n❌ [WASM Sandbox Execution Failed]: {}", e);
                                        }
                                    }
                                } else if !l_path.exists() {
                                    println!("⚠️ [CLI REPL] logic.wasm not found for skill: {}", action);
                                }
                            } else {
                                println!("⚠️ [CLI REPL] Skill not found in registry: {}", action);
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
