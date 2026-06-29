use anyhow::Result;
use std::path::PathBuf;

pub async fn execute() -> Result<()> {
    println!("🧪 [Test] Starting Dynamic Pipeline Diagnostic...");

    let model_path = cluaiz_shared::environment::EnvironmentManager::current().models_dir().join("chat").join("bonsai1-8b").join("Bonsai-8B.gguf");
    
    if !model_path.exists() {
        println!("❌ Model not found at: {:?}", model_path);
        return Ok(());
    }

    // Set Permission.json text chat model to bonsai1:8b and embedding model dynamically from roster
    engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model("bonsai1:8b".to_string());
    let roster = engines::models::registry::CoreRoster::load_roster();
    if let Some(m) = roster.iter().find(|m| (m.architecture_type == "onnx" || m.category == "embedding") && m.local_path.is_some()) {
        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(m.id.clone());
    }



    let dna = cluaiz_shared::StructuralDNA::default();
    let context = cluaiz_shared::cluaizContext::boot(dna, cluaiz_shared::TemplateManager::default());

    println!("⚙️ [Test] Instantiating Chat Engine (Bonsai)...");
    let engine = engines::runtime::execution::hub::HardwareOrchestrator::instantiate(
        model_path.to_str().unwrap(),
        "llama",
        context
    ).await?;

    let prompt = "Make a sad piano instrumental track with slow tempo and emotional vibe";
    println!("🚀 [Test] Triggering stream with prompt: '{}'", prompt);

    // Load active router
    let mut router = engines::api::router::CoreRouter::new();
    let skills_dir = cluaiz_shared::environment::EnvironmentManager::current().skills_dir();
    router.foundry.initialize(&skills_dir.to_string_lossy());
    router.active_backend = engines::api::router::Backend::cluaiz(engine);

    // Boot router indices
    if let Ok(mut g_router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.write() {
        let _ = g_router.boot_index();
    }

    let prompt_str = prompt.to_string();
    let res = tokio::task::block_in_place(|| {
        router.generate_stream(
            &prompt_str,
            5,
            Box::new(|token| {
                print!("{}", token);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                true
            }),
        )
    });

    if res.is_ok() {
        println!("\n✅ [Test] Generation completed!");
        
        let cache_file = cluaiz_shared::environment::EnvironmentManager::current().skills_dir().join("minimax-music-gen").join(".cache").join("bonsai1-8b.kvcache.bin");
        if cache_file.exists() {
            let meta = std::fs::metadata(&cache_file)?;
            let size_mb = meta.len() as f64 / 1_048_576.0;
            println!("✅ [Test] VERIFIED: kvcache.bin was compiled successfully for minimax-music-gen!");
            println!("📁 Cache file location: {:?}", cache_file);
            println!("📊 KV Cache Size: {:.2} MB", size_mb);
            if size_mb > 50.0 {
                println!("❌ [Test] FAILED: KV Cache size is too large ({:.2} MB)!", size_mb);
            } else {
                println!("✅ [Test] PASSED: KV Cache size is minimal ({:.2} MB)!", size_mb);
            }
        } else {
            println!("❌ [Test] FAILED: kvcache.bin was not found!");
        }
    } else {
        println!("❌ [Test] Stream generation failed: {:?}", res.err());
    }

    Ok(())
}
