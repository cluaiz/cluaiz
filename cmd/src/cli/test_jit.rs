use anyhow::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    println!("🧪 [Diagnostics] Starting Native Pipeline Diagnostic...");

    let roster = cluaiz_models::CoreRoster::load_roster();
    let local_model = roster.iter().find(|m| m.local_path.is_some());

    let (model_id, model_path) = match local_model {
        Some(m) => (m.id.clone(), std::path::PathBuf::from(m.local_path.as_ref().unwrap())),
        None => {
            // Check models directory for any downloaded gguf file
            let chat_dir = cluaiz_core::EnvironmentManager::current().models_dir().join("chat");
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(&chat_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        if let Ok(sub) = std::fs::read_dir(&p) {
                            for f in sub.flatten() {
                                if f.path().extension().and_then(|s| s.to_str()) == Some("gguf") {
                                    found = Some((entry.file_name().to_string_lossy().to_string(), f.path()));
                                    break;
                                }
                            }
                        }
                    } else if p.extension().and_then(|s| s.to_str()) == Some("gguf") {
                        found = Some((p.file_name().unwrap_or_default().to_string_lossy().to_string(), p));
                        break;
                    }
                    if found.is_some() {
                        break;
                    }
                }
            }

            match found {
                Some(pair) => pair,
                None => {
                    println!("ℹ️  No local models installed. Run `cluaiz run <model>` first to download a model.");
                    return Ok(());
                }
            }
        }
    };

    if !model_path.exists() {
        println!("❌ Model file not found at: {:?}", model_path);
        return Ok(());
    }

    println!("⚙️ [Diagnostics] Testing engine pipeline with probed model '{}'...", model_id.cyan());
    let host = cluaiz_runtime::EngineHost::new();
    let geometry = cluaiz_models::GgufProber::probe_geometry(&model_path)?;
    let hardware = cluaiz_hw::HardwareProber::current();
    let settings = cluaiz_core::Settings::current();
    let plan = cluaiz_plan::Planner::create_plan(model_path.to_str().unwrap(), &geometry, &hardware, &settings);
    host.load_model(model_path.to_str().unwrap(), &plan, &settings).await?;

    let prompt = "Explain the fundamental principle of computation in one concise sentence.";
    println!("🚀 [Diagnostics] Triggering test dispatch with prompt: '{}'", prompt);

    let resp = host.dispatch_stream(prompt, 32).await;
    match resp {
        cluaiz_runtime::HostResponse::TokenStream(mut rx) => {
            while let Some(token) = rx.recv().await {
                print!("{}", token);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            println!("\n✅ [Diagnostics] Token stream completed successfully!");
        }
        cluaiz_runtime::HostResponse::Completed(text) => {
            print!("{}", text);
            println!("\n✅ [Diagnostics] Generation completed successfully!");
        }
        cluaiz_runtime::HostResponse::Error(e) => {
            println!("❌ [Diagnostics] Generation failed: {:?}", e);
        }
    }

    Ok(())
}
