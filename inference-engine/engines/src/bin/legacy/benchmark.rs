use std::path::PathBuf;
use std::fs::OpenOptions;
use std::io::Write;
use tokenizers::Tokenizer;
use engines::engine::runner::cluaizRunner;
use engines::loader::gguf::GGUFLoader;
use engines::engine::sampler::CoreSampler;

const BENCHMARK_PROMPT: &str =
    "Explain the importance of local AI models and cluaiz Core hardware in one paragraph.";

pub struct BenchmarkTarget {
    pub name: String,
    pub model_path: String,
    pub tokenizer_path: Option<String>,
}

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     🧠 cluaiz: cluaiz Core BENCHMARK v2.0            ║");
    println!("║     Quad-Transformer Faceoff — GPU Powered               ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // ── Hardware Boot ──
    let profile = engines::hardware::cluaizProfile::boot();
    let device = profile.device.clone();
    println!("⚡ Device: {:?}", device);
    println!("⚡ VRAM:   {:.1} GB", profile.vram_gb);
    println!("⚡ RAM:    {:.1} GB", profile.memory.total_ram_gb);
    println!();

    // ── Model Roster ──
    let models_root = r"C:\Users\Aryan\my\cluaiz-workspace\cluaiz-OS\cluaiz-ai\models";
    let tui_models   = r"C:\Users\Aryan\my\cluaiz-workspace\cluaiz-OS\cluaiz-ai\terminal_ui\models";

    let targets = vec![
        BenchmarkTarget {
            name: "Qwen-2.5-0.5B (Baseline)".to_string(),
            model_path: format!(r"{}\models--Qwen--Qwen2.5-0.5B-Instruct-GGUF\qwen2.5-0.5b-instruct-q4_k_m.gguf", models_root),
            tokenizer_path: Some(format!(r"{}\models--Qwen--Qwen2.5-0.5B-Instruct-GGUF\tokenizer.json", models_root)),
        },
        BenchmarkTarget {
            name: "Llama-3.2-1B".to_string(),
            model_path: format!(r"{}\models--lmstudio-community--Llama-3.2-1B-Instruct-GGUF\Llama-3.2-1B-Instruct-Q4_K_M.gguf", models_root),
            tokenizer_path: Some(format!(r"{}\models--lmstudio-community--Llama-3.2-1B-Instruct-GGUF\tokenizer.json", models_root)),
        },
        BenchmarkTarget {
            name: "Gemma-2-2B".to_string(),
            model_path: format!(r"{}\models--lmstudio-community--gemma-2-2b-it-GGUF\gemma-2-2b-it-Q4_K_M.gguf", models_root),
            tokenizer_path: Some(format!(r"{}\models--lmstudio-community--gemma-2-2b-it-GGUF\tokenizer.json", models_root)),
        },
        BenchmarkTarget {
            name: "Qwen3-4B (NEW)".to_string(),
            model_path: format!(r"{}\models--Qwen--Qwen3-4B-GGUF\Qwen3-4B-Q4_K_M.gguf", tui_models),
            tokenizer_path: Some(format!(r"{}\models--Qwen--Qwen3-4B-GGUF\tokenizer.json", tui_models)),
        },
        BenchmarkTarget {
            name: "Bonsai-4B".to_string(),
            model_path: format!(r"{}\models--prism-ml--Bonsai-4B-gguf\Bonsai-4B-Patched.gguf", models_root),
            tokenizer_path: None, // Uses Qwen tokenizer as fallback
        },
    ];

    // ── Report File ──
    let report_path = "Core_Battle_Report.txt";
    let mut report = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(report_path)
        .expect("❌ Could not open report file");

    let header = format!(
        "╔══════════════════════════════════════════════════════════╗\n\
         ║       cluaiz: cluaiz Core BENCHMARK REPORT            ║\n\
         ║       Device: {:?}\n\
         ╚══════════════════════════════════════════════════════════╝\n\n\
         Prompt: \"{}\"\n\
         Max Tokens: 64\n\
         ════════════════════════════════════════════════════════════\n\n",
        device, BENCHMARK_PROMPT
    );
    write!(report, "{}", header).unwrap();

    // ── Qwen tokenizer path for fallback ──
    let fallback_tok_path = format!(
        r"{}\models--Qwen--Qwen2.5-0.5B-Instruct-GGUF\tokenizer.json",
        models_root
    );

    let mut results: Vec<(String, Option<f64>, Option<f64>, Option<f64>)> = Vec::new();

    for (idx, target) in targets.iter().enumerate() {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("[{}/{}] 🔍 Model: {}", idx + 1, targets.len(), target.name);

        let path = PathBuf::from(&target.model_path);
        if !path.exists() {
            println!("   ⚠️  SKIP: File not found → {}", target.model_path);
            writeln!(report, "▶ {}\n   STATUS: SKIPPED (File Not Found)\n", target.name).unwrap();
            results.push((target.name.clone(), None, None, None));
            continue;
        }
        println!("   ✅ Path OK");

        let model = match GGUFLoader::load_model(&path, &device) {
            Ok(m) => {
                println!("   ✅ Loaded");
                m
            }
            Err(e) => {
                println!("   ❌ Load Error: {}", e);
                writeln!(report, "▶ {}\n   STATUS: FAILED ({})\n", target.name, e).unwrap();
                results.push((target.name.clone(), None, None, None));
                continue;
            }
        };

        // ── Tokenizer ──
        let tok_path = target.tokenizer_path.as_deref().unwrap_or(&fallback_tok_path);
        let tokenizer = match Tokenizer::from_file(tok_path) {
            Ok(t) => { println!("   ✅ Tokenizer Ready"); t }
            Err(_) => {
                println!("   ⚠️  Tokenizer not found → using Qwen fallback");
                match Tokenizer::from_file(&fallback_tok_path) {
                    Ok(t) => t,
                    Err(e) => {
                        println!("   ❌ Critical: Fallback tokenizer failed: {}", e);
                        results.push((target.name.clone(), None, None, None));
                        continue;
                    }
                }
            }
        };

        let sampler = CoreSampler::new(299792, 0.7, 0.9, 1.1);
        let mut runner = cluaizRunner::new(model, tokenizer, sampler, None);

        // ── Generation Run ──
        println!("   🚀 Generating...");
        print!("   ▶ ");
        let gen_result = runner.generate(BENCHMARK_PROMPT, 64, |text| {
            print!("{}", text);
            std::io::stdout().flush().unwrap();
        });
        println!();

        match gen_result {
            Ok(m) => {
                println!("   📊 TTFT: {:.2}ms | TPS: {:.2} t/s | Total: {:.0}ms",
                    m.ttft_ms, m.tps, m.total_time_ms);

                writeln!(report,
                    "▶ {}\n   TTFT:       {:.2} ms\n   TPS:        {:.2} tokens/sec\n   Total Time: {:.0} ms\n   Tokens:     {}\n",
                    target.name, m.ttft_ms, m.tps, m.total_time_ms, m.total_tokens
                ).unwrap();

                results.push((target.name.clone(), Some(m.ttft_ms), Some(m.tps), Some(m.total_time_ms)));
            }
            Err(e) => {
                println!("   ❌ Generation Error: {}", e);
                writeln!(report, "▶ {}\n   STATUS: GEN FAILED ({})\n", target.name, e).unwrap();
                results.push((target.name.clone(), None, None, None));
            }
        }

        report.flush().unwrap();
        println!();
    }

    // ── Leaderboard ──
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║                 🏆 Core LEADERBOARD                   ║");
    println!("╠══════════════════════════════════════════════════════════╣");

    writeln!(report, "\n════════════════════════════════════════════════════════════").unwrap();
    writeln!(report, "                   🏆 Core LEADERBOARD").unwrap();
    writeln!(report, "════════════════════════════════════════════════════════════").unwrap();
    writeln!(report, "{:<25} {:>10} {:>12} {:>12}", "Model", "TTFT(ms)", "TPS", "Total(ms)").unwrap();
    writeln!(report, "{}", "─".repeat(62)).unwrap();

    // Sort by TPS descending
    let mut ranked: Vec<_> = results.iter()
        .filter(|(_, _, tps, _)| tps.is_some())
        .collect();
    ranked.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    for (i, (name, ttft, tps, total)) in ranked.iter().enumerate() {
        let medal = match i { 0 => "🥇", 1 => "🥈", 2 => "🥉", _ => "  " };
        let tps_v = tps.unwrap();
        let ttft_v = ttft.unwrap();
        let total_v = total.unwrap();

        println!("║ {} {:<22} {:>8.0}ms {:>8.1} t/s {:>8.0}ms ║",
            medal, &name[..name.len().min(22)], ttft_v, tps_v, total_v);
        writeln!(report, "{} {:<24} {:>10.2} {:>12.2} {:>12.0}",
            medal, name, ttft_v, tps_v, total_v).unwrap();
    }

    println!("╚══════════════════════════════════════════════════════════╝");
    println!("\n💾 Report saved → {}", report_path);
    writeln!(report, "\n════════════════════════════════════════════════════════════").unwrap();
    writeln!(report, "Report saved by cluaiz Benchmark v2.0").unwrap();
    report.flush().unwrap();
}
