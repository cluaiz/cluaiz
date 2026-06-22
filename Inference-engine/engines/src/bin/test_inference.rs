use anyhow::Result;
use engines::runtime::execution::hub::HardwareOrchestrator;
use cluaize_shared::{StructuralDNA, CluaizeContext, TemplateManager};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 [Test] Starting Dynamic Pipeline Diagnostic...");

    let env = cluaize_shared::environment::EnvironmentManager::current();
    let model_path = env.ensure_chat_models_dir()
        .unwrap_or_else(|_| env.chat_models_dir())
        .join("bonsai1-8b").join("Bonsai-8B.gguf");
    
    if !model_path.exists() {
        println!("❌ Model not found at: {:?}", model_path);
        return Ok(());
    }

    // Set Permission.json text chat model to bonsai1:8b
    engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model("bonsai1:8b".to_string());

    let dna = StructuralDNA::default();
    let context = CluaizeContext::boot(dna, TemplateManager::default());

    println!("⚙️ [Test] Instantiating Chat Engine...");
    let mut engine = HardwareOrchestrator::instantiate(
        model_path.to_str().unwrap(),
        "llama",
        context
    ).await?;

    // Load active router
    let mut router = engines::api::router::CoreRouter::new();
    router.active_backend = engines::api::router::Backend::Cluaize(engine);

    let prompt = "Make a sad piano instrumental track with slow tempo and emotional vibe";
    println!("🚀 [Test] Triggering stream with prompt: '{}'", prompt);
    
    let res = router.generate_stream(
        prompt,
        10,
        Box::new(|token| {
            print!("{}", token);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            true
        }),
    );

    if res.is_ok() {
        println!("\n✅ [Test] Generation successful!");
        
        // Let's verify that the kvcache.bin was created for the matched skill
        let cache_file = env.ensure_skills_dir().unwrap_or_else(|_| env.skills_dir()).join("minimax-music-gen").join(".cache").join("bonsai1-8b.kvcache.bin");
        if cache_file.exists() {
            println!("✅ [Test] VERIFIED: kvcache.bin was compiled successfully for minimax-music-gen!");
            println!("📁 Cache file location: {:?}", cache_file);
        } else {
            println!("❌ [Test] FAILED: kvcache.bin was not found!");
        }
    } else {
        println!("❌ [Test] Stream generation failed: {:?}", res.err());
    }

    Ok(())
}
