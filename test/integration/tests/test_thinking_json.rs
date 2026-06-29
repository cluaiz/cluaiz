use anyhow::Result;
use cluaiz_shared::{cluaizContext, StructuralDNA, TemplateManager};
use engines::api::router::{Backend, CoreRouter};
use engines::runtime::execution::hub::HardwareOrchestrator;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Default, Debug)]
struct OutputStructure {
    model_id: String,
    thinking: String,
    answer: String,
    success: bool,
}

#[tokio::test]
async fn test_all_reasoning_models_dynamically() -> Result<()> {
    if let Ok(target_model) = std::env::var("BENCHMARK_MODEL_ID") {
        run_single_model_isolated(&target_model).await;
        return Ok(());
    }

    println!("🧪 [Benchmark] Testing Reasoning Tag Separation on ALL Installed Models...");

    let roster = engines::models::registry::CoreRoster::load_roster();
    let current_exe = std::env::current_exe().expect("Failed to get current executable path");

    for model in roster {
        println!("=======================================================");
        println!("🧪 Spawning Isolated Process for Model: {}", model.id);
        println!("=======================================================");

        let mut cmd = std::process::Command::new(&current_exe);
        // We pass the exact test name so we don't run all tests, but just this one.
        cmd.args(["--nocapture", "--exact", "test_all_reasoning_models_dynamically"])
            .env("BENCHMARK_MODEL_ID", &model.id);

        let status = cmd.status();

        match status {
            Ok(s) if s.success() => {
                println!("✅ Model {} completed successfully.", model.id);
            }
            Ok(s) => {
                println!("❌ Model {} failed with exit code: {}", model.id, s);
            }
            Err(e) => {
                println!("❌ Failed to spawn process for {}: {}", model.id, e);
            }
        }

        println!("🧹 Parent process waiting 3s for OS to absolutely flush VRAM...");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }

    println!("✅ Benchmark Complete. All models tested.");
    Ok(())
}

async fn run_single_model_isolated(model_name: &str) {
    let folder_name = model_name.replace(':', "-");
    let models_dir = cluaiz_shared::environment::EnvironmentManager::current()
        .get_models_dir()
        .join("chat");
    let model_folder = models_dir.join(&folder_name);

    if !model_folder.exists() {
        println!("⏭️ Skipping {} because folder does not exist.", model_name);
        std::process::exit(0);
    }

    let mut gguf_file_path = String::new();
    if let Ok(entries) = std::fs::read_dir(&model_folder) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("gguf") {
                gguf_file_path = p.to_string_lossy().to_string();
                break;
            }
        }
    }

    if gguf_file_path.is_empty() {
        println!("⏭️ Skipping {} because no .gguf file was found in {:?}", model_name, model_folder);
        std::process::exit(0);
    }

    // Check DNA
    let dna = match StructuralDNA::load(&model_folder) {
        Ok(d) => d,
        Err(e) => {
            println!("⚠️ Could not load DNA for {}, error: {}. Probing...", model_name, e);
            let mut d = StructuralDNA::default();
            d.model_identity = model_name.to_string();
            let _ = d.discover_from_path(&model_folder);
            d
        }
    };

    let mut start_tag = String::new();
    let mut end_tag = String::new();

    if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" {
        start_tag = dna.think_tag_schema.clone();
        end_tag = dna.think_end_schema.clone();
    }

    println!("🧠 Using tags - Start: '{}', End: '{}'", start_tag, end_tag);
    println!("⏳ Instantiating Model in VRAM... (Please wait)");

    let context = cluaizContext::boot(dna.clone(), TemplateManager::default());
    let engine_result = HardwareOrchestrator::instantiate(
        &gguf_file_path,
        "llama",
        context
    ).await;

    if let Ok(engine) = engine_result {
        let mut router = CoreRouter::new();
        router.active_backend = Backend::cluaiz(engine);

        let prompt = "Think step by step and explain why the sky is blue. Keep it very short.";
        println!("📥 Prompt: '{}'", prompt);

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        
        let prompt_clone = prompt.to_string();
        tokio::task::spawn_blocking(move || {
            let _ = router.generate_stream(
                &prompt_clone,
                2048,
                Box::new(move |token| {
                    let _ = tx.send(token);
                    true
                })
            );
        });

        let mut output = String::new();
        while let Some(token) = rx.recv().await {
            print!("{}", token);
            let _ = std::io::stdout().flush();
            output.push_str(&token);
        }

        println!("\n\n✅ Done generating for {}.", model_name);

        let mut thinking = String::new();
        let mut answer = output.clone();

        let mut effective_start = start_tag.clone();
        let mut effective_end = end_tag.clone();

        // Only use the DNA's effective tags. Do not fall back to hallucinated tags
        // if the model's DNA tells us exactly what tag to expect.
        if !effective_start.is_empty() && !output.contains(&effective_start) {
            println!("⚠️ DNA start tag '{}' not found in output. It might be empty or hallucinated.", effective_start);
        }

        if !effective_start.is_empty() {
            if let Some(start_idx) = output.find(&effective_start) {
                if let Some(end_idx) = output.find(&effective_end) {
                    let think_content = &output[start_idx + effective_start.len()..end_idx];
                    thinking = think_content.trim().to_string();
                    answer = output[end_idx + effective_end.len()..].trim().to_string();
                } else {
                    // Model hallucinated or forgot to close the thought block.
                    // If it forgot to close, it's safer to treat the entire block as the answer
                    // rather than having an empty answer block.
                    thinking = String::new();
                    answer = output[start_idx + effective_start.len()..].trim().to_string();
                }
            }
        }


        let result = OutputStructure {
            model_id: model_name.to_string(),
            thinking,
            answer,
            success: true,
        };

        // Append to benchmark_output.json
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("benchmark_output.jsonl") {
            if let Ok(json) = serde_json::to_string(&result) {
                let _ = writeln!(file, "{}", json);
            }
        }
    } else {
        println!("❌ Failed to instantiate engine for {}", model_name);
        std::process::exit(1);
    }

    std::process::exit(0);
}

// FORCE RECOMPILE
