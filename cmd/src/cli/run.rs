use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::io::{self, Write, Read};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// `cluaiz run <model-id>` — pulls the model and initiates a native chat session.
pub async fn execute(model_id: &str, _interactive: bool, _all: bool) -> Result<()> {
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

    // Extract optional explicit quantization/variant tag if specified with colon (e.g. 'repo/model:IQ3_XXS' or 'model:Q4_K_M')
    let (base_input, explicit_tag) = if let Some((base, tag)) = model_id.rsplit_once(':') {
        if !tag.is_empty() && !base.ends_with("http") && !base.ends_with("https") && !base.ends_with("hf") {
            (base, Some(tag.to_string()))
        } else {
            (model_id, None)
        }
    } else {
        (model_id, None)
    };

    let mut target_hf_repo: Option<String> = None;
    if !base_input.contains('/') && !base_input.starts_with("hf://") && !base_input.starts_with("https://") {
        // Check if model already exists locally
        let safe_id = model_id.replace(':', "-");
        let safe_base_id = base_input.replace(':', "-");
        let local_exists = roster.iter().any(|m| {
            if m.id.to_lowercase() == model_id.to_lowercase() || m.id.to_lowercase() == base_input.to_lowercase() {
                let model_path = cluaiz_root.join(&m.category).join(&safe_id);
                let base_path = cluaiz_root.join(&m.category).join(&safe_base_id);
                model_path.join(&m.huggingface_filename).exists() || base_path.join(&m.huggingface_filename).exists()
            } else {
                false
            }
        });

        if !local_exists {
            // Short ID given and not present locally -> Resolve upstream Hugging Face repo from Cluaiz Registry
            print!("  {} Resolving model ID '{}' via Cluaiz Registry...", "🔍".cyan(), base_input.bold());
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let resolved_res = match engines::models::fetch::resolve_model_repo(model_id).await {
                Ok(Some(repo)) => Ok(Some(repo)),
                _ => engines::models::fetch::resolve_model_repo(base_input).await,
            };

            if let Ok(Some(hf_repo)) = resolved_res {
                println!(" -> Found HuggingFace Repo: '{}'", hf_repo.yellow().bold());
                target_hf_repo = Some(hf_repo);
            } else {
                println!(" (using local/external catalog)");
            }
        }
    }

    if base_input.contains('/') || target_hf_repo.is_some() {
        // 🚀 HUGGINGFACE REQUEST (EXPLICIT OR RESOLVED)
        is_hf = true;
        resolved_id = target_hf_repo.unwrap_or_else(|| base_input.to_string());
        if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") {
            resolved_id = format!("hf://{}", resolved_id);
        }
        
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo_id);
        
        let selected_variant = if _all {
            println!("  {} Fetching full repository file tree for '{}'...", "🌐".cyan(), repo_id);
            let raw_tree = engines::models::manager::hf_hub::HuggingFaceHub::list_raw_tree(&repo_id).await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;

            if raw_tree.is_empty() {
                return Err(color_eyre::eyre::eyre!("No files found in repository '{}'", repo_id));
            }

            let selected_files = run_directory_tree_picker(&repo_id, &raw_tree)?;

            if selected_files.is_empty() {
                return Err(color_eyre::eyre::eyre!("No files selected for download."));
            }

            let primary_file = selected_files.iter()
                .find(|f| f.ends_with(".gguf") || f.ends_with(".onnx") || f.ends_with(".safetensors"))
                .cloned()
                .unwrap_or_else(|| selected_files[0].clone());

            let total_size_bytes: u64 = selected_files.iter()
                .filter_map(|f| raw_tree.iter().find(|item| &item.path == f).and_then(|i| i.size))
                .sum();
            let size_gb = total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            engines::models::manager::hf_hub::HfVariant {
                variant_id: "UNFILTERED CUSTOM BUNDLE".to_string(),
                format_type: if primary_file.ends_with(".onnx") { "onnx".to_string() } else { "gguf".to_string() },
                quant_tag: "CUSTOM".to_string(),
                primary_file: primary_file.clone(),
                all_files: selected_files,
                filename: primary_file,
                size_gb,
            }
        } else {
            let variants = engines::models::manager::hf_hub::HuggingFaceHub::list_variants(&repo_id).await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;

            if variants.is_empty() {
                return Err(color_eyre::eyre::eyre!("No variants found in repository '{}'", repo_id));
            }

            // ⚡ If an explicit quantization tag was given (e.g. :IQ3_XXS or :Q4_K_M), auto-select it directly!
            let matched_from_tag = if let Some(ref tag) = explicit_tag {
                let tag_upper = tag.to_uppercase();
                let clean_tag = tag_upper.replace(['-', '_'], "");
                
                variants.iter().find(|v| {
                    v.quant_tag.to_uppercase() == tag_upper
                        || v.variant_id.to_uppercase().contains(&tag_upper)
                        || v.filename.to_uppercase().contains(&tag_upper)
                        || v.quant_tag.to_uppercase().replace(['-', '_'], "") == clean_tag
                        || v.filename.to_uppercase().replace(['-', '_'], "").contains(&clean_tag)
                }).cloned()
            } else {
                None
            };

            if let Some(matched) = matched_from_tag {
                println!(
                    "  {} Direct Quantization Tag Matched: ':{}' -> {} ({:.2} GB)",
                    "⚡".green().bold(),
                    explicit_tag.as_ref().unwrap().yellow().bold(),
                    matched.variant_id.cyan().bold(),
                    matched.size_gb
                );
                matched
            } else {
                if let Some(ref tag) = explicit_tag {
                    println!(
                        "  {} Quantization tag ':{}' not found in '{}'. Available variants listed below:",
                        "⚠️".yellow(),
                        tag.bold(),
                        repo_id.cyan()
                    );
                }

                let is_noninteractive = std::env::var("CLUAIZ_NONINTERACTIVE").is_ok();
                if is_noninteractive {
                    variants.first().ok_or_else(|| color_eyre::eyre::eyre!("No variants found"))?.clone()
                } else {
                    let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.variant_id, v.size_gb)).collect();
                    let selected_option = inquire::Select::new("Select model variant bundle to download:", options)
                        .with_page_size(12)
                        .raw_prompt()
                        .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?;
                    
                    variants.into_iter().nth(selected_option.index)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Selected variant not found"))?
                }
            }
        };

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
        let hf_manifest = engines::models::HuggingFaceHub::build_manifest(&repo_id, &selected_variant, None);
            
        manifest = Some(hf_manifest);
        selected_variant_bundle = Some(selected_variant);
        is_local = false;

        // Append quant tag to manifest ID for flat vault folder naming
        // e.g. GLM-5.2-GGUF + UD-IQ1_S → GLM-5.2-GGUF-UD-IQ1_S (flat, not nested)
        if let (Some(ref mut m), Some(ref bundle)) = (&mut manifest, &selected_variant_bundle) {
            let qt = bundle.quant_tag.clone();
            if !qt.is_empty() && qt != "DEFAULT" {
                let qt_upper = qt.to_uppercase();
                let id_upper = m.id.to_uppercase();
                if !id_upper.contains(&qt_upper) {
                    m.id = format!("{}-{}", m.id, qt);
                }
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

    // 🚀 Trigger Tools Engine to sync and provision tools registry
    let _ = engines::tools::ToolsEngine::registry();

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

        let is_noninteractive = std::env::var("CLUAIZ_NONINTERACTIVE").is_ok();
        let confirm = if is_noninteractive {
            true
        } else {
            inquire::Confirm::new("\nProceed with model download?").with_default(true).prompt()?
        };
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
                            
                            let mut skill_name = None;
                            let mut logic_path = None;
                            let mut is_allowed = false;
                            
                            {
                                if let Ok(Some(tool)) = engines::tools::ToolsEngine::get_tool(action) {
                                    let router = state.Core_engine.router.lock().await;
                                    let tool_path = std::path::PathBuf::from(&tool.local_dir);
                                    skill_name = Some(tool.name.clone());
                                    logic_path = Some(tool_path.join("logic.wasm"));
                                    is_allowed = router.foundry.guard.validate_action(&tool.id, engines::neural_foundry::security::guard::PermissionLevel::ReadOnly).is_ok();
                                }
                            }
                            
                            if let Some(name) = skill_name {
                                let l_path = logic_path.unwrap();
                                if is_allowed && l_path.exists() {
                                    println!("⚙️ [CLI REPL] Executing WASM Sandbox for: {}", name.green());
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

#[derive(Debug, Clone)]
enum TreeItemNode {
    ParentDir,
    Folder {
        name: String,
        full_path: String,
        selected_count: usize,
        total_count: usize,
    },
    File {
        name: String,
        full_path: String,
        size_bytes: u64,
        is_selected: bool,
    },
}

fn run_directory_tree_picker(repo_id: &str, raw_items: &[engines::models::manager::hf_hub::HfTreeItem]) -> Result<Vec<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType};
    use crossterm::cursor::{MoveTo, Hide, Show};
    use crossterm::execute;
    use std::collections::HashSet;

    let mut stdout = io::stdout();
    let mut selected_files: HashSet<String> = HashSet::new();
    let mut current_dir = String::new();
    let mut cursor_idx = 0;

    let all_files: Vec<(String, u64)> = raw_items.iter()
        .filter(|item| item.r#type.as_deref() != Some("directory"))
        .map(|item| (item.path.clone(), item.size.unwrap_or(0)))
        .collect();

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let result = (|| -> Result<Vec<String>> {
        loop {
            let mut folders: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
            let mut current_files: Vec<(String, String, u64)> = Vec::new();

            let prefix = if current_dir.is_empty() {
                "".to_string()
            } else if current_dir.ends_with('/') {
                current_dir.clone()
            } else {
                format!("{}/", current_dir)
            };

            for (path, size) in &all_files {
                if prefix.is_empty() || path.starts_with(&prefix) {
                    let rel = &path[prefix.len()..];
                    if let Some(slash_pos) = rel.find('/') {
                        let folder_name = &rel[..slash_pos];
                        let entry = folders.entry(folder_name.to_string()).or_insert((0, 0));
                        entry.1 += 1;
                        if selected_files.contains(path) {
                            entry.0 += 1;
                        }
                    } else {
                        current_files.push((rel.to_string(), path.clone(), *size));
                    }
                }
            }

            let mut nodes: Vec<TreeItemNode> = Vec::new();

            if !current_dir.is_empty() {
                nodes.push(TreeItemNode::ParentDir);
            }

            for (f_name, (sel_cnt, tot_cnt)) in folders {
                let full = if prefix.is_empty() {
                    f_name.clone()
                } else {
                    format!("{}{}", prefix, f_name)
                };
                nodes.push(TreeItemNode::Folder {
                    name: f_name,
                    full_path: full,
                    selected_count: sel_cnt,
                    total_count: tot_cnt,
                });
            }

            current_files.sort_by(|a, b| a.0.cmp(&b.0));
            for (f_name, full, size) in current_files {
                let is_sel = selected_files.contains(&full);
                nodes.push(TreeItemNode::File {
                    name: f_name,
                    full_path: full,
                    size_bytes: size,
                    is_selected: is_sel,
                });
            }

            if cursor_idx >= nodes.len() && !nodes.is_empty() {
                cursor_idx = nodes.len() - 1;
            }

            // Move cursor to top-left for zero-flicker drawing
            execute!(stdout, MoveTo(0, 0))?;

            let mut render_buf = String::new();

            let breadcrumb = if current_dir.is_empty() {
                format!("  📁 Root: {}", repo_id.bold().cyan())
            } else {
                format!("  📁 Location: {}", format!("{}/{}", repo_id, current_dir).bold().cyan())
            };

            render_buf.push_str(&format!("\x1B[K\r\n{}\x1B[K\r\n  {}\x1B[K\r\n", breadcrumb, "─".repeat(70).dimmed()));

            let total_selected_size: u64 = all_files.iter()
                .filter(|(p, _)| selected_files.contains(p))
                .map(|(_, s)| *s)
                .sum();
            let total_gb = total_selected_size as f64 / (1024.0 * 1024.0 * 1024.0);

            let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 40));
            // Header = 3 lines, Footer = 3 lines => 6 lines overhead + 1 safety
            let page_size = (term_height as usize).saturating_sub(7).max(5);
            let start_win = if cursor_idx >= page_size { cursor_idx - page_size + 1 } else { 0 };
            let end_win = (start_win + page_size).min(nodes.len());
            let visible_rows = end_win - start_win;

            let thumb_row = if nodes.len() > visible_rows {
                (cursor_idx * visible_rows) / nodes.len().max(1)
            } else {
                99999
            };

            for (row_idx, idx) in (start_win..end_win).enumerate() {
                let is_cursor = idx == cursor_idx;
                let pointer = if is_cursor { ">".green().bold().to_string() } else { " ".to_string() };
                let scroll_char = if nodes.len() > visible_rows {
                    if row_idx == thumb_row { "#".cyan().bold().to_string() } else { "|".dimmed().to_string() }
                } else {
                    " ".to_string()
                };

                match &nodes[idx] {
                    TreeItemNode::ParentDir => {
                        let parent_str = "[..] Parent Directory".cyan().bold();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {}\x1B[K\r\n", pointer, parent_str.on_black(), scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {}\x1B[K\r\n", pointer, parent_str, scroll_char));
                        }
                    }
                    TreeItemNode::Folder { name, selected_count, total_count, .. } => {
                        let check = if *selected_count == *total_count && *total_count > 0 {
                            "[✓]".green().bold()
                        } else if *selected_count > 0 {
                            "[-]".yellow().bold()
                        } else {
                            "[ ]".dimmed()
                        };
                        let folder_str = format!("{}/", name).cyan().bold();
                        let stats = format!("({}/{})", selected_count, total_count).dimmed();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, folder_str.on_black(), stats, scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, folder_str, stats, scroll_char));
                        }
                    }
                    TreeItemNode::File { name, size_bytes, is_selected, .. } => {
                        let check = if *is_selected { "[✓]".green().bold() } else { "[ ]".dimmed() };
                        let size_mb = *size_bytes as f64 / (1024.0 * 1024.0);
                        let size_str = format!("({:.1} MB)", size_mb).dimmed();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, name.yellow(), size_str, scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, name, size_str, scroll_char));
                        }
                    }
                }
            }

            render_buf.push_str(&format!("  {}\x1B[K\r\n", "─".repeat(60).dimmed()));
            render_buf.push_str(&format!("  Selected: {} files ({:.2} GB)\x1B[K\r\n", selected_files.len().to_string().green().bold(), total_gb));
            render_buf.push_str(&format!("  {}  {}  {}  {}  {}  {}  {}\x1B[K\r\n", 
                "⌨".white().bold(), "↕:Up/Dn".yellow(), "↩:Enter".cyan(), "␣:Space".magenta(), "←:Back".blue(), "Y:Get".green().bold(), "Q:Exit".red().bold()));

            write!(stdout, "{}", render_buf)?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Up => {
                            if cursor_idx > 0 {
                                cursor_idx -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if cursor_idx + 1 < nodes.len() {
                                cursor_idx += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if cursor_idx < nodes.len() {
                                match &nodes[cursor_idx] {
                                    TreeItemNode::ParentDir => {
                                        if let Some(last_slash) = current_dir.rfind('/') {
                                            current_dir = current_dir[..last_slash].to_string();
                                        } else {
                                            current_dir.clear();
                                        }
                                        cursor_idx = 0;
                                        execute!(stdout, Clear(ClearType::All))?;
                                    }
                                    TreeItemNode::Folder { full_path, .. } => {
                                        current_dir = full_path.clone();
                                        cursor_idx = 0;
                                        execute!(stdout, Clear(ClearType::All))?;
                                    }
                                    TreeItemNode::File { full_path, is_selected, .. } => {
                                        if *is_selected {
                                            selected_files.remove(full_path);
                                        } else {
                                            selected_files.insert(full_path.clone());
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Left | KeyCode::Esc => {
                            if !current_dir.is_empty() {
                                if let Some(last_slash) = current_dir.rfind('/') {
                                    current_dir = current_dir[..last_slash].to_string();
                                } else {
                                    current_dir.clear();
                                }
                                cursor_idx = 0;
                                execute!(stdout, Clear(ClearType::All))?;
                            } else {
                                return Err(color_eyre::eyre::eyre!("Selection cancelled by user"));
                            }
                        }
                        KeyCode::Char(' ') => {
                            if cursor_idx < nodes.len() {
                                match &nodes[cursor_idx] {
                                    TreeItemNode::ParentDir => {}
                                    TreeItemNode::Folder { full_path, .. } => {
                                        let folder_prefix = format!("{}/", full_path);
                                        let child_files: Vec<String> = all_files.iter()
                                            .filter(|(p, _)| p.starts_with(&folder_prefix))
                                            .map(|(p, _)| p.clone())
                                            .collect();
                                        
                                        let all_sel = child_files.iter().all(|f| selected_files.contains(f));
                                        if all_sel {
                                            for f in child_files {
                                                selected_files.remove(&f);
                                            }
                                        } else {
                                            for f in child_files {
                                                selected_files.insert(f);
                                            }
                                        }
                                    }
                                    TreeItemNode::File { full_path, is_selected, .. } => {
                                        if *is_selected {
                                            selected_files.remove(full_path);
                                        } else {
                                            selected_files.insert(full_path.clone());
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Tab => {
                            if selected_files.is_empty() {
                                continue;
                            }
                            let mut sorted: Vec<String> = selected_files.into_iter().collect();
                            sorted.sort();
                            return Ok(sorted);
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            return Err(color_eyre::eyre::eyre!("Selection cancelled by user"));
                        }
                        KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            return Err(color_eyre::eyre::eyre!("Aborted by user"));
                        }
                        _ => {}
                    }
                }
            }
        }
    })();

    let _ = execute!(stdout, LeaveAlternateScreen, Show);
    let _ = disable_raw_mode();
    result
}
