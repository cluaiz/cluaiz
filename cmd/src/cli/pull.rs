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

    let mut resolved_id = base_input.to_string();
    if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") {
        if resolved_id.contains('/') {
            resolved_id = format!("hf://{}", resolved_id);
        } else {
            // Short ID given (e.g. 'qwen:0.6b' or 'whisper-base') -> Query Cluaiz Model Resolver API for upstream Hugging Face repo
            print!("  {} Resolving model ID '{}' via Cluaiz Registry...", "🔍".cyan(), base_input.bold());
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let resolved_res = match engines::models::fetch::resolve_model_repo(model_id).await {
                Ok(Some(repo)) => Ok(Some(repo)),
                _ => engines::models::fetch::resolve_model_repo(base_input).await,
            };

            if let Ok(Some(hf_repo)) = resolved_res {
                println!(" -> Found HuggingFace Repo: '{}'", hf_repo.yellow().bold());
                resolved_id = format!("hf://{}", hf_repo);
            } else {
                println!(" (using local/external catalog)");
            }
        }
    }

    if resolved_id.starts_with("hf://") || resolved_id.starts_with("https://huggingface.co/") {
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo_id);
        
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

        let selected_variant = if let Some(matched) = matched_from_tag {
            println!(
                "  {} Direct Quantization Tag Matched: ':{}' -> {} ({:.2} GB)",
                "⚡".green().bold(),
                explicit_tag.as_ref().unwrap().yellow().bold(),
                matched.filename.cyan().bold(),
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

            let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.filename, v.size_gb)).collect();
            let selection = if std::env::var("CLUAIZ_NON_INTERACTIVE").is_ok() {
                if options.is_empty() {
                    return Err(color_eyre::eyre::eyre!("No variants found to download."));
                }
                println!("  [Non-Interactive Mode] Auto-selecting first variant: {}", options[0]);
                options[0].clone()
            } else {
                inquire::Select::new("Select GGUF variant to download:", options).prompt()
                    .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?
            };
            
            // Extract filename
            let selected_filename = selection.split(" (").next().unwrap().to_string();
            variants.into_iter().find(|v| v.filename == selected_filename)
                .ok_or_else(|| color_eyre::eyre::eyre!("Variant not found"))?
        };

        let selected_filename = selected_variant.filename.clone();
        
        let roster = engines::models::registry::CoreRoster::load_roster();
        if let Some(existing) = roster.iter().find(|m| m.huggingface_repo.to_lowercase() == repo_id.to_lowercase() && m.huggingface_filename.to_lowercase() == selected_filename.to_lowercase()) {
            println!("\n  {} Warning: This exact variant is already downloaded locally under ID: '{}'", "⚠️".yellow(), existing.id.cyan());
            println!("     If you wish to re-download, please delete the old one first using: cluaiz rm {}\n", existing.id.red());
            return Ok(());
        }

        println!("  {} Fetching precise metadata...", "📡".cyan());
        let hf_manifest = engines::models::HuggingFaceHub::build_manifest(&repo_id, &selected_variant, None);
            
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
    
    println!("  {} Silicon Pre-flight Architecture:", "📡".cyan());
    println!("    ├─ 🧠 Family / Name: {}", manifest.name.yellow());
    println!("    ├─ 📁 Sovereign Category: {}", manifest.category.green());
    println!("    ├─ 📏 Context Window: {} tokens", manifest.context_window.cyan());
    println!("    ├─ 🧮 RAM Projected: {:.2} GB", manifest.ram_required_gb);
    println!("    ├─ 💾 Download Size: {:.2} GB", manifest.download_size_gb);

    // Perform system health audit silently in the background
    let total_required = manifest.ram_required_gb;
    let status = manager.audit_model_health(total_required as f32, manifest.requires_gpu);

    // 🛑 Pre-flight Quantization Check
    if manifest.bit_depth > 0.0 && manifest.bit_depth < 3.0 {
        println!("\n  {} [Pre-flight Warning] Unsupported Quantization Detected!", "⚠️".yellow());
        println!("     This model uses {:.2}-bit quantization (e.g., Q2_0 or BitNet).", manifest.bit_depth);
        println!("     The current C++ backend may crash when attempting to load these weights.");
        println!("     The download will proceed, but expect 'invalid ggml type' errors at runtime.");
    }
    
    if status == engines::models::manager::auditor::HealthStatus::Disabled {
        return Err(color_eyre::eyre::eyre!("❌ DENIED: Insufficient hardware resources for this model."));
    } else {
        let confirm = if std::env::var("CLUAIZ_NON_INTERACTIVE").is_ok() {
            println!("  [Non-Interactive Mode] Auto-confirming model initialization.");
            true
        } else {
            inquire::Confirm::new("Audit passed. All metadata exposed. Proceed with model initialization?").with_default(true).prompt()?
        };
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

