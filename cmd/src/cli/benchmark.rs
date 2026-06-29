use cluaiz_shared::backend::traits::{cluaizInference, UnifiedBackend};
use cluaiz_shared::hardware::governor::HardwareGovernor;
use cluaiz_shared::hardware::schema::booster::FeatureState;
use cluaiz_shared::{cluaizContext, StructuralDNA, TemplateManager};
use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;
use engines::runtime::execution::hub::HardwareOrchestrator;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Deserialize, Serialize, Clone)]
struct BenchmarkPrompt {
    id: String,
    category: String,
    difficulty: String,
    text: String,
    min_model_size_b: usize,
}

#[derive(Deserialize, Serialize)]
struct BenchmarkSuite {
    prompts: Vec<BenchmarkPrompt>,
}

pub async fn execute(target_model_id: Option<String>, runs: usize) -> Result<()> {
    println!(
        "\n  {} [Performance] Starting Full System Benchmark...\n",
        "🚀".magenta()
    );

    // Check if we are running in process-isolated mode
    if let Ok(target_model) = std::env::var("BENCHMARK_MODEL_ID") {
        let env_runs = std::env::var("BENCHMARK_RUNS")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<usize>()
            .unwrap_or(1);
        run_single_model_isolated(&target_model, env_runs).await;
        return Ok(());
    }

    let full_roster = CoreRoster::load_roster();
    
    // Filter to only downloaded models
    let mut roster = Vec::new();
    for model in full_roster {
        let path_str = if let Some(local_path) = &model.local_path {
            local_path.clone()
        } else {
            let folder_name = model.id.replace(':', "-");
            let models_dir = cluaiz_shared::environment::EnvironmentManager::current()
                .models_dir()
                .join("chat");
            models_dir.join(&folder_name).to_string_lossy().to_string()
        };
        let model_folder = PathBuf::from(path_str);
        if find_gguf_file(&model_folder).is_some() {
            roster.push(model);
        }
    }
    
    if roster.is_empty() {
        println!(
            "     {} No downloaded models found in the vault to benchmark.",
            "⚠️ ".yellow()
        );
        return Ok(());
    }

    let targets = if let Some(id) = target_model_id {
        roster
            .into_iter()
            .filter(|m| m.id == id)
            .collect::<Vec<_>>()
    } else {
        // Interactive Selection Prompt
        let mut options = vec!["All Downloaded Models".to_string()];
        for m in &roster {
            options.push(m.id.clone());
        }

        let choice = inquire::Select::new("Select model to benchmark:", options).prompt()?;
        
        if choice == "All Downloaded Models" {
            roster
        } else {
            roster
                .into_iter()
                .filter(|m| m.id == choice)
                .collect::<Vec<_>>()
        }
    };

    if targets.is_empty() {
        println!(
            "     {} Selected model not found in the vault.",
            "⚠️ ".yellow()
        );
        return Ok(());
    }

    let mode_choice = inquire::Select::new(
        "Select Benchmark Mode:",
        vec![
            "🚀 Auto Suite (Run all predefined prompts)",
            "✍️ Custom Prompt (Enter your own prompt)"
        ]
    ).prompt()?;

    let custom_prompt_val = if mode_choice.contains("Custom Prompt") {
        let p = inquire::Text::new("Enter your custom prompt:").prompt()?;
        Some(p)
    } else {
        None
    };

    let out_dir = get_benchmark_out_dir();
    fs::create_dir_all(&out_dir).unwrap_or_default();

    println!(
        "  {} Found {} models in the vault. Preparing benchmark orchestrator...\n",
        "📊".blue(),
        targets.len()
    );

    let current_exe = std::env::current_exe().expect("Failed to get current executable path");

    for model in &targets {
        println!("=======================================================");
        println!(
            "🧪 Spawning Isolated Process for Model: {}",
            model.id.green()
        );
        println!("=======================================================");

        let folder_arg = if let Some(local_path) = &model.local_path {
            local_path.clone()
        } else {
            model.id.replace(':', "-")
        };

        let mut cmd = std::process::Command::new(&current_exe);
        cmd.args(["benchmark"])
            .env("BENCHMARK_MODEL_ID", &model.id)
            .env("BENCHMARK_FOLDER_NAME", folder_arg)
            .env("BENCHMARK_RUNS", runs.to_string());
            
        if let Some(ref p) = custom_prompt_val {
            cmd.env("BENCHMARK_CUSTOM_PROMPT", p);
        }

        let status = cmd.status();

        match status {
            Ok(s) if s.success() => {
                println!("✅ Model {} completed successfully.", model.id);
            }
            Ok(s) if s.code() == Some(2) => {
                println!("⏭️ Model {} skipped (Missing GGUF weights).", model.id);
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

    println!(
        "\n  {} All benchmarks completed! Reports are in {:?}\n",
        "🎉".green(),
        out_dir
    );
    Ok(())
}

fn get_benchmark_out_dir() -> PathBuf {
    let mut base = if cfg!(debug_assertions) {
        // Development Environment
        let mut path = std::env::current_dir().unwrap_or_default();
        while let Some(name) = path.file_name() {
            if name.to_string_lossy() == "cluaiz" {
                break;
            }
            if !path.pop() {
                break;
            }
        }
        path.join("test").join("benchmark")
    } else {
        // Production Environment
        cluaiz_shared::environment::EnvironmentManager::current()
            .local_dir
            .join("reports")
            .join("benchmark")
    };

    if let Ok(control) = HardwareGovernor::load_system_control() {
        let compute = if control.silicon_truth.accelerators.gpus.is_empty() {
            "CPU_ONLY".to_string()
        } else {
            "GPU".to_string()
        };

        let hardware_name = if compute == "GPU" {
            control.silicon_truth.accelerators.gpus[0]
                .model
                .replace(" ", "_")
                .replace("/", "_")
        } else {
            control
                .silicon_truth
                .cpu
                .brand
                .replace(" ", "_")
                .replace("/", "_")
        };

        base.join(compute).join(hardware_name)
    } else {
        base.join("Unknown_Hardware")
    }
}

fn find_gguf_file(dir: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                return Some(path);
            }
        }
    }
    None
}

async fn run_single_model_isolated(model_name: &str, runs: usize) {
    let folder_name =
        std::env::var("BENCHMARK_FOLDER_NAME").unwrap_or_else(|_| model_name.replace(':', "-"));

    let path_str = if folder_name.contains('/') || folder_name.contains('\\') {
        folder_name.clone()
    } else {
        let models_dir = cluaiz_shared::environment::EnvironmentManager::current()
            .models_dir()
            .join("chat");
        models_dir.join(&folder_name).to_string_lossy().to_string()
    };

    let model_folder = PathBuf::from(path_str);
    let out_dir = get_benchmark_out_dir();

    let gguf_file = match find_gguf_file(&model_folder) {
        Some(file) => file,
        None => {
            println!(
                "⚠️ No .gguf file found for {} at {:?}",
                model_name, model_folder
            );
            std::process::exit(2);
        }
    };

    let suite = if let Ok(custom_prompt_text) = std::env::var("BENCHMARK_CUSTOM_PROMPT") {
        BenchmarkSuite {
            prompts: vec![BenchmarkPrompt {
                id: "custom_prompt".to_string(),
                category: "Custom".to_string(),
                difficulty: "Variable".to_string(),
                text: custom_prompt_text,
                min_model_size_b: 0,
            }],
        }
    } else {
        // Load suite JSON from embedded file
        let suite_content = include_str!("../../../test/benchmark_suite.json");
        serde_json::from_str::<BenchmarkSuite>(suite_content)
            .unwrap_or_else(|_| BenchmarkSuite { prompts: vec![] })
    };

    if suite.prompts.is_empty() {
        println!("⚠️ No prompts available in benchmark suite.");
        return;
    }

    // DYNAMIC DNA: Try to load structural DNA from the local path if available
    let dna_path = model_folder.join("structural_dna.json");
    let mut dna = StructuralDNA::default();
    if dna_path.exists() {
        if let Ok(dna_content) = fs::read_to_string(&dna_path) {
            if let Ok(parsed_dna) = serde_json::from_str::<StructuralDNA>(&dna_content) {
                dna = parsed_dna;
            } else {
                dna.model_identity = model_name.to_string();
                let _ = dna.discover_from_path(&model_folder);
            }
        }
    } else {
        dna.model_identity = model_name.to_string();
        let _ = dna.discover_from_path(&model_folder);
    }

    let mut think_start_tag = String::new();
    let mut think_end_tag = String::new();
    if !dna.think_tag_schema.is_empty() && dna.think_tag_schema != "none" {
        think_start_tag = dna.think_tag_schema.clone();
        think_end_tag = dna.think_end_schema.clone();
    }

    // Allow HardwareGovernor to negotiate context length dynamically based on VRAM
    // dna.max_context_length is left untouched to represent the model's true architecture limit.

    let context = cluaizContext::boot(dna.clone(), TemplateManager::default());

    // Model Params Guess (for prompt filtering)
    let approx_params_b = (dna.weights_size_gb * 1.0) as usize; // Roughly 1GB = 1B params for q4/q8. Simple heuristic.

    println!("🔥 Booting Engine in Process Isolation...");
    let mut engine = match engines::runtime::execution::hub::HardwareOrchestrator::instantiate(
        gguf_file.to_str().unwrap(),
        "llama",
        context,
    ).await {
            Ok(engine) => engine,
            Err(e) => {
                println!(
                    "❌ Failed to instantiate engine for {}: {:?}",
                    model_name, e
                );
                std::process::exit(1);
            }
        };

    println!("🔥 Warming up {}...", model_name);
    let warmup_prompts = vec!["Hello", "Test"];

    for prompt in warmup_prompts {
        let _ = engine.generate_stream(prompt, 5, Box::new(|_| { true }));
    }

    println!("⚡ Warmup complete. Running main benchmark...");
    
    let registry = HardwareGovernor::load_process_registry();
    let my_pid = std::process::id().to_string();
    let (vram_used_gb, context_size) = if let Some(info) = registry.get(&my_pid) {
        (info.vram_gb, info.context_size)
    } else {
        (approx_params_b as f64 * 0.8, dna.max_context_length.unwrap_or(8192))
    };

    // Create report directory early
    let safe_folder_name = model_name.replace(':', "-").replace('/', "_");
    let report_dir = out_dir.join(&safe_folder_name);
    fs::create_dir_all(&report_dir).unwrap_or_default();

    for prompt_def in suite.prompts {
        if approx_params_b < prompt_def.min_model_size_b {
            println!(
                "⏭️ Skipping Prompt [{}] - Target Model Size Not Supported.",
                prompt_def.id
            );
            continue;
        }

        println!(
            "\n  🧪 Prompt: [{}] ({})",
            prompt_def.id, prompt_def.difficulty
        );

        let mut prompt_report_md = format!(
            "# 🚀 cluaiz Hardware Benchmark Report\n\n\
            ## 🤖 Model: {}\n\
            ### 🛠️ Hardware & Environment\n\
            - **Compute Node**: {:?}\n\
            - **Approx. Parameters**: ~{}B\n\
            - **Context Window**: {} (Dynamic Limit)\n\
            - **VRAM Used**: {:.2} GB\n\n\
            ## 📊 Benchmark Results\n\n\
            ### 🧪 [{}] - {} ({})\n\
            **Prompt**: `{}`\n\n",
            model_name,
            get_benchmark_out_dir(),
            approx_params_b,
            context_size,
            vram_used_gb,
            prompt_def.id,
            prompt_def.category,
            prompt_def.difficulty,
            prompt_def.text
        );

        // Loop through Thinking Off and Thinking On
        for think_mode in [false, true] {
            println!(
                "     🧠 Thinking Mode: {}",
                if think_mode { "ON" } else { "OFF" }
            );

            // Reconfigure booster
            let mut booster = HardwareGovernor::load_booster_settings().unwrap_or_default();
            booster.think_mode = if think_mode {
                FeatureState::On
            } else {
                FeatureState::Off
            };
            let _ = HardwareGovernor::save_booster_settings(&booster);

            let mut highest_tps = 0.0;
            let mut best_run_output = String::new();
            let mut best_time = 0.0;
            let mut best_tokens = 0;
            let mut best_ttft = 0.0;

            for i in 1..=runs {
                println!("        [Run {}/{}] Generating...", i, runs);
                let start = Instant::now();
                let mut generated_text = String::new();

                let (tx, mut rx) = mpsc::unbounded_channel();
                let first_token_time = Arc::new(Mutex::new(None));
                let first_token_time_clone = first_token_time.clone();
                let token_count = Arc::new(AtomicUsize::new(0));
                let token_count_clone = token_count.clone();

                let result = engine.generate_stream(
                    &prompt_def.text,
                    2048,
                    Box::new(move |token| {
                        let mut lock = first_token_time_clone.lock().unwrap();
                        if lock.is_none() {
                            *lock = Some(start.elapsed().as_secs_f64());
                        }
                        token_count_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(token);
                        true
                    }),
                );

                if let Err(ref e) = result {
                    println!("        ❌ [Run {}/{}] FAILED: {:?}", i, runs, e);
                    continue;
                }

                while let Ok(token) = rx.try_recv() {
                    generated_text.push_str(&token);
                }

                let elapsed = start.elapsed().as_secs_f64();
                let tokens = token_count.load(Ordering::Relaxed);
                let tps = if elapsed > 0.0 {
                    tokens as f64 / elapsed
                } else {
                    0.0
                };
                let ttft = first_token_time.lock().unwrap().unwrap_or(0.0);

                println!(
                    "          ⏱️ Time: {:.2}s | 🧩 Tokens: {} | 🚀 TPS: {:.2} | ⏱️ TTFT: {:.2}s",
                    elapsed, tokens, tps, ttft
                );

                if tps > highest_tps {
                    highest_tps = tps;
                    best_run_output = generated_text;
                    best_time = elapsed;
                    best_tokens = tokens;
                    best_ttft = ttft;
                }
            }

            prompt_report_md.push_str(&format!(
                "#### 🧠 Thinking Mode: {}\n",
                if think_mode { "ON" } else { "OFF" }
            ));
            prompt_report_md.push_str(&format!("- **Speed**: {:.2} TPS\n", highest_tps));
            prompt_report_md.push_str(&format!("- **TTFT**: {:.2} s\n", best_ttft));
            prompt_report_md.push_str(&format!("- **Tokens**: {}\n", best_tokens));
            prompt_report_md.push_str(&format!("- **Time**: {:.2} s\n\n", best_time));
            prompt_report_md.push_str(&format!(
                "**Output:**\n{}\n\n",
                format_model_output(&best_run_output, &think_start_tag, &think_end_tag)
            ));
        }

        let mut counter = 0;
        let mut file_name = format!("{}.md", prompt_def.id);
        let mut file_path = report_dir.join(&file_name);

        while file_path.exists() {
            counter += 1;
            file_name = format!("{}-{}.md", prompt_def.id, counter);
            file_path = report_dir.join(&file_name);
        }

        fs::write(&file_path, prompt_report_md).unwrap_or_default();
        println!("📝 Report for [{}] saved to {:?}", prompt_def.id, file_path);
    }
}

fn format_model_output(raw: &str, think_start_tag: &str, think_end_tag: &str) -> String {
    if think_start_tag.is_empty() {
        return raw.trim().to_string();
    }

    let mut result = String::new();
    let mut current_idx = 0;
    
    while let Some(start) = raw[current_idx..].find(think_start_tag) {
        let absolute_start = current_idx + start;
        
        // Add anything before <think>
        if absolute_start > current_idx {
            let before = &raw[current_idx..absolute_start].trim();
            if !before.is_empty() {
                result.push_str(before);
                result.push_str("\n\n");
            }
        }
        
        let search_start = absolute_start + think_start_tag.len();
        if !think_end_tag.is_empty() {
            if let Some(end) = raw[search_start..].find(think_end_tag) {
                let absolute_end = search_start + end;
                let think_content = &raw[search_start..absolute_end].trim();
                
                result.push_str("> **🧠 Thinking Process:**\n");
                for line in think_content.lines() {
                    result.push_str(&format!("> {}\n", line));
                }
                result.push_str("\n\n");
                
                current_idx = absolute_end + think_end_tag.len();
            } else {
                // Missing closing tag, treat rest of string as thinking
                let think_content = &raw[search_start..].trim();
                result.push_str("> **🧠 Thinking Process (Incomplete):**\n");
                for line in think_content.lines() {
                    result.push_str(&format!("> {}\n", line));
                }
                result.push_str("\n\n");
                current_idx = raw.len();
                break;
            }
        } else {
            let think_content = &raw[search_start..].trim();
            result.push_str("> **🧠 Thinking Process (No End Tag):**\n");
            for line in think_content.lines() {
                result.push_str(&format!("> {}\n", line));
            }
            result.push_str("\n\n");
            current_idx = raw.len();
            break;
        }
    }
    
    // Add whatever is left
    if current_idx < raw.len() {
        let remaining = &raw[current_idx..].trim();
        if !remaining.is_empty() {
            result.push_str(remaining);
        }
    }
    
    if result.is_empty() {
        raw.trim().to_string()
    } else {
        result
    }
}
