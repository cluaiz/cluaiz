use std::path::PathBuf;
use engines::{GGUFLoader, cluaizRunner};
use engines::engine::CoreSampler;
use engines::hardware::cluaizProfile;
use engines::telemetry::health_check::cluaizHealthChecker;
use engines::core::{JitAllocator, ExecutionTier};
use tokenizers::Tokenizer;

#[tokio::main]
async fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("🚀 [cluaiz] cluaiz Core IGNITION (ONBOARDING MODE)");
    println!("═══════════════════════════════════════════════════════════════\n");

    // ── PHASE 1: HARDWARE DISCOVERY ──
    println!("🔍 Phase 1: Hardware Topological Discovery...");
    let mut profile = cluaizProfile::boot();
    
    // ── PHASE 2: MACRO HEALTH BENCHMARKS (RAM & STORAGE) ──
    println!("\n🩺 Phase 2: Macro Health Benchmarking...");
    profile = cluaizHealthChecker::execute_initial_diagnostic(profile);
    
    // ── PHASE 3: JIT ORCHESTRATION DECISION ──
    println!("\n🧠 Phase 3: JIT Brain Allocation Decision...");
    let tier = JitAllocator::determine_execution_graph(&profile);
    
    match tier {
        ExecutionTier::Tier1Parallel => {
            println!("✨ [AUTO-CONFIG] High-Power Configuration: Hot-Pinning all models.");
        }
        ExecutionTier::Tier2Sequential => {
            println!("⚠️ [AUTO-CONFIG] Balanced Configuration: JIT Swapping enabled.");
        }
        ExecutionTier::Tier3EdgeFallback => {
            println!("🔴 [AUTO-CONFIG] Edge Mode: Minimal context & Aggressive pruning.");
        }
    }

    // ── PHASE 4: MODEL LOADING & INFERENCE ──
    println!("\n⏳ Initiating cluaiz Dispatcher (Tier Mode: {:?})...", tier);
    
    let model_path = PathBuf::from(r"C:\Users\Aryan\my\cluaiz-workspace\cluaiz-OS\cluaiz-ai\models\models--Qwen--Qwen2.5-0.5B-Instruct-GGUF\qwen2.5-0.5b-instruct-q4_k_m.gguf");
    
    if !model_path.exists() {
        println!("❌ Warning: Model not found at: {}. Skipping generation check.", model_path.display());
        return;
    }

    // SND Loader using detected device
    let model = match GGUFLoader::load_model(&model_path, &profile.device) {
        Ok(m) => m,
        Err(e) => {
            println!("❌ FATAL: SND Loader Error: {}", e);
            return;
        }
    };

    let tokenizer_path = r"C:\Users\Aryan\my\cluaiz-workspace\Cluaiz\cluaiz-ai\models\models--Qwen--Qwen2.5-0.5B-Instruct-GGUF\tokenizer.json";
    let tokenizer = Tokenizer::from_file(tokenizer_path).expect("Tokenizer load error");
    
    let sampler = CoreSampler::new(299792, 0.7, 0.9, 1.1);
    let mut runner = cluaizRunner::new(model, tokenizer, sampler, None);

    println!("\n✅ SUCCESS: cluaiz Onboarding Complete. System Stable.");
    println!("🤖 Assistant Prompted: \"Describe the soul of a cluaiz AI .\"");
    println!("\n═════════════════- 🧠 cluaiz BRAIN -═════════════════\n");

    let prompt = "<|im_start|>user\nDescribe the soul of a cluaiz AI  in one short sentence.<|im_end|>\n<|im_start|>assistant\n";
    
    match runner.generate(prompt, 64, |text| {
        print!("{}", text);
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }) {
        Ok(metrics) => {
            println!("\n\n════════════════════════════════════════════════");
            println!("📊 [PERF] TTFT: {:.2}ms | TPS: {:.2} tokens/sec", metrics.ttft_ms, metrics.tps);
        }
        Err(e) => println!("\n❌ GENERATION ERROR: {}", e),
    }

    println!("\n🏁 IGNITION SEQUENCE DONE. SYSTEM READY.");
}
