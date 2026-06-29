use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;

/// `cluaiz run <model-id>` — pulls the model and initiates a native chat session.
pub async fn execute(model_id: &str) -> Result<()> {
    // 🎨 Display the Sovereign Logo
    let logo = crate::assets::logos::logo_gallery::LOGO_VARIANTS[9];
    println!("{}", logo.cyan());

    println!("\n  {} [cluaiz] Initializing Kernel for '{}'...", "⚙️".yellow(), model_id.bold());

    // 1. Resolve Metadata
    let mut manifest: Option<engines::models::registry::ModelManifest> = None;
    
    // Auto-detect HuggingFace repos like 'unsloth/Qwen3.5-4B-GGUF'
    let mut resolved_id = model_id.to_string();
    if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") && resolved_id.contains('/') {
        resolved_id = format!("hf://{}", resolved_id);
    }

    if resolved_id.starts_with("hf://") || resolved_id.starts_with("https://huggingface.co/") {
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo_id);
        
        let variants = engines::models::manager::hf_hub::HuggingFaceHub::list_variants(&repo_id).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.filename, v.size_gb)).collect();
        let selection = inquire::Select::new("Select GGUF variant to download:", options).prompt()
            .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?;
            
        // Extract filename and size
        let selected_filename = selection.split(" (").next().unwrap().to_string();
        
        let roster = engines::models::registry::CoreRoster::load_roster();
        if let Some(existing) = roster.iter().find(|m| m.huggingface_repo.to_lowercase() == repo_id.to_lowercase() && m.huggingface_filename.to_lowercase() == selected_filename.to_lowercase()) {
            println!("\n  {} Warning: This exact variant is already downloaded locally under ID: '{}'", "⚠️".yellow(), existing.id.cyan());
            println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", existing.id.red());
            return Ok(());
        }

        let selected_size_str = selection.split(" (").nth(1).unwrap().replace(" GB)", "");
        let selected_size_gb: f64 = selected_size_str.parse().unwrap_or(0.0);
        
        println!("  {} Fetching precise metadata...", "📡".cyan());
        let hf_manifest = engines::models::manager::hf_hub::HuggingFaceHub::build_manifest(&repo_id, &selected_filename, selected_size_gb).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
            
        manifest = Some(hf_manifest);
    } else {
        let roster = CoreRoster::load_roster();
        manifest = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());

        if manifest.is_none() {
            println!("  {} Model missing in local vault. Synchronizing with Neural Registry...", "🌐".yellow());
            let remote_models = CoreRoster::fetch_external_registry(None).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }
    }

    let mut manifest = manifest.ok_or_else(|| color_eyre::eyre::eyre!("ID '{}' not found in any registry.", model_id))?;

    // Check if it's already downloaded!
    let cached_path = engines::models::fetch::ModelDownloader::get_cached_path(&manifest.category, &manifest.id, &manifest.huggingface_filename);
    if cached_path.is_some() {
        println!("\n  {} Warning: Model '{}' is already downloaded locally.", "⚠️".yellow(), manifest.id.cyan());
        println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", manifest.id.red());
        return Ok(());
    }

    // 2. Pre-flight Silicon Audit (Universal for both HF and Registry)
    let cluaiz_root = cluaiz_shared::environment::EnvironmentManager::current()
        .ensure_models_dir()
        .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());
    let manager = engines::models::manager::ModelManager::new(engines::models::registry::REGISTRY_URL.to_string(), cluaiz_root.clone());
    
    println!("  {} Fetching Deep Metadata (GGUF Binary Probe)...", "📡".cyan());
    
    if let Ok((metadata, _tensor_infos, tensor_count)) = engines::models::manager::hf_hub::HuggingFaceHub::fetch_partial_gguf_metadata(&manifest.download_url).await {
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
        // 2 bytes per float16, 2 for K and V tensors
        let kv_cache_bytes = 2 * 2 * num_layers * num_kv_heads * head_dim * standard_context_tokens;
        let kv_cache_gb = kv_cache_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let base_engine_overhead_gb = 0.30;
        
        // Dynamically override manifest RAM based on exact architectural math
        manifest.ram_required_gb = manifest.download_size_gb + base_engine_overhead_gb + kv_cache_gb;

        println!("    ├─ 🧠 Architecture: {}", arch.yellow());
        println!("    ├─ 📏 Context Window: {} tokens", ctx_display.green());
        println!("    ├─ 🧩 Parameters: {} B", params.green());
        println!("    ├─ 📦 Quantization / File Type: {}", file_type.cyan());
        println!("    ├─ 📚 Network Layers (Blocks): {}", blocks.magenta());
        println!("    ├─ ⚡ Tensor Count: {}", tensor_count.to_string().cyan());
        println!("    ├─ 💾 Download Size: {:.2} GB", manifest.download_size_gb);
        println!("    ├─ 🧮 KV Cache (8K tokens): {:.2} GB", kv_cache_gb);
        println!("    ├─ ⚙️ Base Engine Overhead: {:.2} GB", base_engine_overhead_gb);
    } else if let Err(e) = engines::models::manager::hf_hub::HuggingFaceHub::fetch_partial_gguf_metadata(&manifest.download_url).await {
        println!("    ├─ ⚠️ Could not probe remote GGUF header: {}", e);
        println!("    ├─ 💾 Download Size: {:.2} GB", manifest.download_size_gb);
    }

    println!("\n  {} Conducting Pre-flight Silicon Audit...", "⚖️".cyan());
    
    // Load System Control to get real hardware stats
    let config_path = cluaiz_shared::hardware::governor::HardwareGovernor::resolve_engine_path().join("system_control.json");
    let system_control = if let Ok(content) = std::fs::read_to_string(config_path) {
        serde_json::from_str::<cluaiz_shared::hardware::schema::profiles::SystemControl>(&content).ok()
    } else {
        None
    };

    let (mut user_ram, mut user_vram) = (0.0, 0.0);
    if let Some(control) = &system_control {
        user_ram = control.silicon_truth.memory.total_capacity_gb;
        user_vram = control.silicon_truth.accelerators.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
    }
    
    println!("    ├─ 🖥️ Host System RAM: {:.2} GB", user_ram);
    if user_vram > 0.0 {
        println!("    ├─ 🎮 Target VRAM (Primary GPU): {:.2} GB", user_vram);
    }

    let total_required = manifest.ram_required_gb;
    println!("    ├─ 📊 Target Allocation: {:.2} GB (Weights + Engine + 8K Context)", total_required);

    // 🛑 Pre-flight Quantization Check
    if manifest.bit_depth > 0.0 && manifest.bit_depth < 3.0 {
        println!("\n  {} [Pre-flight Warning] Unsupported Quantization Detected!", "⚠️".yellow());
        println!("     This model uses {:.2}-bit quantization (e.g., Q2_0 or BitNet).", manifest.bit_depth);
        println!("     The current C++ backend may crash when attempting to load these weights.");
        println!("     The download will proceed, but expect 'invalid ggml type' errors at runtime.");
    }

    let mut projected_tps = 0.0;
    
    if user_vram > 0.0 {
        if total_required <= user_vram {
            println!("    ├─ ⚡ Offload Status: Full GPU Acceleration (100% VRAM)");
            println!("    ├─ 🧮 Remaining VRAM post-load: {:.2} GB", user_vram - total_required);
            projected_tps = 35.0; 
        } else {
            let vram_ratio = user_vram / total_required;
            println!("    ├─ ⚡ Offload Status: Partial GPU Acceleration ({:.0}% in VRAM)", vram_ratio * 100.0);
            println!("    ├─ 🧮 Remaining System RAM post-load: {:.2} GB", user_ram - (total_required - user_vram));
            
            if vram_ratio > 0.8 { projected_tps = 22.0; }
            else if vram_ratio > 0.5 { projected_tps = 15.0; }
            else { projected_tps = 8.0; }
        }
    } else {
        println!("    ├─ ⚡ Offload Status: CPU Inference (No dedicated VRAM)");
        println!("    ├─ 🧮 Remaining System RAM post-load: {:.2} GB", user_ram - total_required);
        projected_tps = 5.0;
    }

    println!("    ├─ 🚀 Projected Speed: ~{:.0} Tokens/Second (TPS)", projected_tps);

    let status = manager.audit_model_health(total_required as f32, manifest.requires_gpu);
    println!("    ├─ System Status: {:?}", status);
    
    if status == engines::models::manager::auditor::HealthStatus::Disabled {
        return Err(color_eyre::eyre::eyre!("❌ DENIED: Insufficient hardware resources for this model."));
    } else {
        let confirm = inquire::Confirm::new("Audit passed. All metadata exposed. Proceed with model initialization?").with_default(true).prompt()?;
        if !confirm {
            return Err(color_eyre::eyre::eyre!("Initialization aborted by user."));
        }
    }

    // 3. Provision Weights
    if resolved_id.starts_with("hf://") || resolved_id.starts_with("https://huggingface.co/") {
        manager.pull_model_with_manifest(&manifest).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
        println!("\n  {} HuggingFace Model Downloaded Successfully!", "✅".green());
    } else {
        manager.pull_model(&resolved_id).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
    }

    // 3. Hardware Orchestration (The Neural Bridge)
    let safe_id = manifest.id.replace(':', "-");
    let model_path = cluaiz_root.join(&manifest.category).join(&safe_id);
    let model_file = model_path.join(&manifest.huggingface_filename);
    
    if !model_file.exists() {
        return Err(color_eyre::eyre::eyre!("Model file not found at: {:?}", model_file));
    }

    let dna = cluaiz_shared::StructuralDNA::default();
    let context = cluaiz_shared::cluaizContext::boot(dna, cluaiz_shared::TemplateManager::default());

    let engine = engines::runtime::execution::hub::HardwareOrchestrator::instantiate(
        model_file.to_str().unwrap(),
        "gguf",
        context
    ).await.map_err(|e| color_eyre::eyre::eyre!(e))?;

    println!("  {} Handshake Success. Entering Dashboard...\n", "✅".green());

    // 4. Launch Dashboard in Sovereign Mode (Pre-loaded with the model)
    use crate::core::state::AppState;
    use tokio::sync::mpsc;
    
    let mut state = AppState::new(None);
    // Pre-load the engine into the state
    {
        let mut lock = state.Core_engine.router.lock().await;
        lock.active_backend = engines::api::router::Backend::cluaiz(engine);
    }
    state._active_model_id = Some(manifest.id.clone());

    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut mode = crate::app_enums::Mode::Running;

    //  Start the Dashboard UI
    crate::core::dashboard::DashboardEngine::run_native(
        &mut state,
        &tx,
        &mut rx,
        &mut mode
    )?;

    Ok(())
}

