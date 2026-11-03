//! `cluaiz bench`: llama-bench's speed test on the engine. Prompt processing (`pp N`: N random tokens
//! in batches) and generation (`tg N`: N tokens one by one), each repeated, reported as tokens/s
//! mean ± standard deviation — the numbers are comparable with llama-bench run on the same model.
//!
//! The generation and prompt speeds are kept for the placement they ran on (`cluaiz_plan::speed`); with
//! `--placements` every placement the planner can use is measured, and the planner then loads the
//! fastest one (planner STEP 4).

use std::sync::Arc;

use cluaiz_core::ActiveUserSettings;
use cluaiz_plan::{speed, ExecutionPlan};
use color_eyre::{eyre::eyre, Result};

use crate::cli::args::LoadArgs;

#[derive(clap::Args)]
pub struct BenchArgs {
    /// Installed model id or path to a .gguf file
    pub model: String,
    /// Prompt lengths to test (comma separated, 0 = skip)
    #[arg(short = 'p', long = "n-prompt", value_name = "N", value_delimiter = ',', default_value = "512")]
    pub n_prompt: Vec<usize>,
    /// Generation lengths to test (comma separated, 0 = skip)
    #[arg(short = 'n', long = "n-gen", value_name = "N", value_delimiter = ',', default_value = "128")]
    pub n_gen: Vec<usize>,
    /// Runs per test
    #[arg(short = 'r', long = "repetitions", value_name = "N", default_value_t = 5)]
    pub repetitions: usize,
    /// Test every placement the planner can use for this model (full GPU, split, CPU only…); the
    /// planner then loads the fastest one
    #[arg(long = "placements")]
    pub placements: bool,
    /// Output format
    #[arg(short = 'o', long = "output", value_enum, default_value_t = Output::Md)]
    pub output: Output,
    #[command(flatten)]
    pub load: LoadArgs,
}

#[derive(clap::ValueEnum, Clone, Copy, PartialEq)]
pub enum Output {
    Md,
    Json,
}

struct Row {
    placement: &'static str,
    n_gpu_layers: i32,
    n_cpu_moe: u32,
    n_ctx: u32,
    test: String,
    tokens: usize,
    samples_ns: Vec<u64>,
}

impl Row {
    /// tokens/s of each run, as llama-bench computes avg_ts / stdev_ts
    fn rates(&self) -> Vec<f64> {
        self.samples_ns.iter().map(|ns| self.tokens as f64 * 1e9 / *ns as f64).collect()
    }

    fn mean_sd(&self) -> (f64, f64) {
        let r = self.rates();
        let n = r.len() as f64;
        let mean = r.iter().sum::<f64>() / n;
        let sd = if r.len() > 1 { (r.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
        (mean, sd)
    }
}

/// The settings that load exactly `candidate` of `plan` (its blocks, experts, KV place, and the
/// plan's context and KV type, so every placement is measured on the same context)
fn pinned(settings: &ActiveUserSettings, plan: &ExecutionPlan, candidate: &cluaiz_plan::ExecutionCandidate) -> ActiveUserSettings {
    let mut pinned = settings.clone();
    let opt = &mut pinned.optimization;
    opt.n_gpu_layers = Some(candidate.n_gpu_layers);
    opt.n_cpu_moe = Some(candidate.n_cpu_moe);
    opt.offload_kqv = Some(candidate.kv_offload);
    opt.set_context_setting(cluaiz_core::ContextSetting::Custom(plan.context_window));
    opt.kv_cache_quantization = plan.kv_cache_type;
    pinned
}

pub async fn execute(args: BenchArgs, verbose: bool) -> Result<()> {
    let (path, id) = crate::cli::models::resolve_installed(&args.model)?;
    let mut settings = cluaiz_core::Settings::current();
    args.load.apply(&mut settings.optimization).map_err(|e| eyre!(e))?;

    let mut tests: Vec<(String, usize, usize)> = Vec::new();
    tests.extend(args.n_prompt.iter().filter(|n| **n > 0).map(|n| (format!("pp{}", n), *n, 0)));
    tests.extend(args.n_gen.iter().filter(|n| **n > 0).map(|n| (format!("tg{}", n), 0, *n)));
    if tests.is_empty() {
        return Err(eyre!("nothing to test: give -p and/or -n greater than 0"));
    }
    if args.placements && settings.optimization.is_manual_placement() {
        return Err(eyre!("--placements tests the planner's placements; leave out --n-gpu-layers, --n-cpu-moe and --no-kv-offload"));
    }

    let host = Arc::new(cluaiz_runtime::EngineHost::new());
    let spin = crate::ui::spinner::Spinner::start("Loading model…");
    let plan = host.ensure_loaded(&id, &path, &settings).await.map_err(|e| eyre!("{}", e))?;
    drop(spin);
    let geometry = cluaiz_models::GgufProber::probe_geometry(&path).map_err(|e| eyre!("{}", e))?;
    let desc = host.model_description().await.ok_or_else(|| eyre!("model did not load"))?;

    // The loaded placement first, then (--placements) every other one that fits in memory
    let mut runs: Vec<Option<ActiveUserSettings>> = vec![None];
    if args.placements {
        let mut seen = vec![(plan.n_gpu_layers, plan.n_cpu_moe, plan.kv_offload)];
        for c in plan.candidates.iter().filter(|c| c.feasible && c.disk_stream_gb == 0.0) {
            let placement = (c.n_gpu_layers, c.n_cpu_moe, c.kv_offload);
            if !seen.contains(&placement) {
                seen.push(placement);
                runs.push(Some(pinned(&settings, &plan, c)));
            }
        }
    }

    let mut rows = Vec::new();
    for run in runs {
        let plan = match run {
            None => plan.clone(),
            Some(pinned) => {
                let spin = crate::ui::spinner::Spinner::start("Loading the next placement…");
                let loaded = host.ensure_loaded(&id, &path, &pinned).await;
                drop(spin);
                match loaded {
                    Ok(plan) => plan,
                    Err(e) => {
                        eprintln!("skipped a placement: {}", e);
                        continue;
                    }
                }
            }
        };
        if verbose {
            for line in plan.summary().lines() {
                eprintln!("  {}", line);
            }
        }
        for (test, n_prompt, n_gen) in &tests {
            eprintln!("running {} on {} ({} runs)...", test, plan.placement_label(), args.repetitions.max(1));
            let samples_ns = host.bench(*n_prompt, *n_gen, args.repetitions).await.map_err(|e| eyre!("{}: {}", test, e))?;
            let row = Row {
                placement: plan.placement_label(),
                n_gpu_layers: plan.n_gpu_layers,
                n_cpu_moe: plan.n_cpu_moe,
                n_ctx: plan.context_window,
                test: test.clone(),
                tokens: n_prompt + n_gen,
                samples_ns,
            };
            // What the planner compares: the first generation test and the first prompt test of
            // each placement
            let test_kind = if *n_gen > 0 { speed::Test::Generation } else { speed::Test::Prompt };
            let tag = &test[..2];
            if !rows.iter().any(|r: &Row| r.test.starts_with(tag) && r.n_gpu_layers == plan.n_gpu_layers
                && r.n_cpu_moe == plan.n_cpu_moe && r.placement == plan.placement_label())
            {
                let signature = speed::Signature {
                    tier: plan.selected_tier,
                    n_gpu_layers: plan.n_gpu_layers,
                    n_cpu_moe: plan.n_cpu_moe,
                    kv_offload: plan.kv_offload,
                };
                speed::record(test_kind, &geometry, &settings, plan.context_window, plan.kv_cache_type, &signature, row.mean_sd().0);
            }
            rows.push(row);
        }
    }
    cluaiz_runtime::vram_use::save();

    let size_gib = desc.size_bytes as f64 / cluaiz_core::GIB_F64;
    let params_b = desc.n_params as f64 / 1e9;
    match args.output {
        Output::Md => {
            println!("| model | size | params | placement | ngl | n_ctx | test | t/s |");
            println!("| ----- | ---: | -----: | --------- | --: | ----: | ---: | --: |");
            for r in &rows {
                let (mean, sd) = r.mean_sd();
                println!(
                    "| {} | {:.2} GiB | {:.2} B | {} | {} | {} | {} | {:.2} ± {:.2} |",
                    desc.desc, size_gib, params_b, r.placement, r.n_gpu_layers, r.n_ctx, r.test, mean, sd
                );
            }
            if args.placements {
                println!("\nThe planner now loads the fastest of these placements for {} (planner STEP 4).", id);
            }
        }
        Output::Json => {
            let out: Vec<_> = rows
                .iter()
                .map(|r| {
                    let (mean, sd) = r.mean_sd();
                    serde_json::json!({
                        "model": desc.desc, "model_id": id, "size_bytes": desc.size_bytes, "n_params": desc.n_params,
                        "placement": r.placement, "n_gpu_layers": r.n_gpu_layers, "n_cpu_moe": r.n_cpu_moe,
                        "n_ctx": r.n_ctx, "test": r.test, "samples_ns": r.samples_ns, "samples_ts": r.rates(),
                        "avg_ts": mean, "stdev_ts": sd,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }
    Ok(())
}
