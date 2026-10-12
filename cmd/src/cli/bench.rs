//! `cluaiz bench`: llama-bench's speed test on the engine. Prompt processing (`pp N`: N random tokens
//! in batches) and generation (`tg N`: N tokens one by one), each repeated, reported as tokens/s
//! mean ± standard deviation — the numbers are comparable with llama-bench run on the same model.

use std::sync::Arc;

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

pub async fn execute(args: BenchArgs, verbose: bool) -> Result<()> {
    let (path, id) = crate::cli::models::resolve_installed(&args.model)?;
    let mut settings = cluaiz_core::Settings::current();
    args.load.apply(&mut settings.optimization).map_err(|e| eyre!(e))?;

    let host = Arc::new(cluaiz_runtime::EngineHost::new());
    let plan = host.ensure_loaded(&id, &path, &settings).await.map_err(|e| eyre!("{}", e))?;
    if verbose {
        for line in plan.summary().lines() {
            eprintln!("  {}", line);
        }
    }
    let desc = host.model_description().await.ok_or_else(|| eyre!("model did not load"))?;

    let mut tests: Vec<(String, usize, usize)> = Vec::new();
    tests.extend(args.n_prompt.iter().filter(|n| **n > 0).map(|n| (format!("pp{}", n), *n, 0)));
    tests.extend(args.n_gen.iter().filter(|n| **n > 0).map(|n| (format!("tg{}", n), 0, *n)));
    if tests.is_empty() {
        return Err(eyre!("nothing to test: give -p and/or -n greater than 0"));
    }

    let mut rows = Vec::new();
    for (test, n_prompt, n_gen) in tests {
        eprintln!("running {} ({} runs)...", test, args.repetitions.max(1));
        let samples_ns = host.bench(n_prompt, n_gen, args.repetitions).await.map_err(|e| eyre!("{}: {}", test, e))?;
        rows.push(Row { test, tokens: n_prompt + n_gen, samples_ns });
    }

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
                    desc.desc, size_gib, params_b, plan.placement_label(), plan.n_gpu_layers, plan.context_window, r.test, mean, sd
                );
            }
        }
        Output::Json => {
            let out: Vec<_> = rows
                .iter()
                .map(|r| {
                    let (mean, sd) = r.mean_sd();
                    serde_json::json!({
                        "model": desc.desc, "model_id": id, "size_bytes": desc.size_bytes, "n_params": desc.n_params,
                        "placement": plan.placement_label(), "n_gpu_layers": plan.n_gpu_layers, "n_cpu_moe": plan.n_cpu_moe,
                        "n_ctx": plan.context_window, "test": r.test, "samples_ns": r.samples_ns, "samples_ts": r.rates(),
                        "avg_ts": mean, "stdev_ts": sd,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
    }
    Ok(())
}
