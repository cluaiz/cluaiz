//! `cluaiz bench <model> --mtp-compare`: MTP speculative decoding off and on, on real text, on the same
//! placement. llama-bench's `tg` test generates from random tokens, where the model's own guesses
//! mean nothing, so MTP is measured by answering prompts through the normal chat path (`Session`),
//! greedy and without thinking: generation and prompt speed per answer, the share of guesses kept,
//! and whether the text is the same with and without MTP.
//!
//! The prompts are data (`data/bench_mtp_prompts.json`, or `--prompts <file>` in the same form). The
//! results are kept with the placement speeds (`cluaiz_plan::speed`, tests `text` and `text-mtp`) for
//! the Auto rules to come (engine/llama/README.md, "MTP speculative decoding").

use std::sync::Arc;

use cluaiz_core::{FeatureState, KvCacheQuantization};
use cluaiz_plan::{speed, ExecutionPlan};
use color_eyre::{eyre::eyre, Result};

use crate::chat::{output::Output, ChatOptions, Session};
use crate::cli::args::{GpuLayers, LoadArgs};
use crate::cli::bench::{BenchArgs, Output as Format};

const DEFAULT_PROMPTS: &str = include_str!("data/bench_mtp_prompts.json");

#[derive(serde::Deserialize)]
struct Prompt {
    name: String,
    text: String,
    max_tokens: usize,
}

/// The answer text, nothing printed
#[derive(Default)]
struct Collect(String);

impl Output for Collect {
    fn content(&mut self, text: &str) {
        self.0.push_str(text);
    }
    fn reasoning(&mut self, _text: &str) {}
    fn tick(&mut self) {}
    fn finish(&mut self) {}
}

/// One answer: speeds as the Engine reports them (with MTP the stream times the generation itself)
#[derive(Clone, Default)]
struct Answer {
    text: String,
    tokens: i32,
    generation_tps: f64,
    prompt_tps: f64,
    drafted: u64,
    accepted: u64,
}

struct Pair {
    prompt: String,
    off: Vec<Answer>,
    on: Vec<Answer>,
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let v: Vec<f64> = values.collect();
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// llama.cpp's cache type name of the plan's KV type (None = let the planner choose)
fn cache_type(kv: KvCacheQuantization) -> Option<String> {
    match kv {
        KvCacheQuantization::Kv16 => Some("f16".into()),
        KvCacheQuantization::Kv8 => Some("q8_0".into()),
        KvCacheQuantization::Kv4 => Some("q4_0".into()),
        KvCacheQuantization::Auto => None,
    }
}

/// The load flags that give exactly `plan`'s placement, context and KV type, with MTP on or off
fn pinned(base: &LoadArgs, plan: &ExecutionPlan, mtp: bool) -> LoadArgs {
    let mut load = base.clone();
    load.n_gpu_layers = Some(GpuLayers::Count(plan.n_gpu_layers));
    load.n_cpu_moe = Some(plan.n_cpu_moe);
    load.no_kv_offload = !plan.kv_offload;
    load.ctx_size = Some(plan.context_window as i64);
    load.cache_type_k = cache_type(plan.kv_cache_type);
    load.cache_type_v = load.cache_type_k.clone();
    load.mtp = Some(if mtp { "on" } else { "off" }.into());
    load
}

async fn answer(session: &Session, host: &cluaiz_runtime::EngineHost, prompt: &Prompt) -> Result<Answer> {
    // Every run processes its whole prompt, so prompt speed is measured too (the drafter reads
    // every prompt chunk, which is MTP's cost on long prompts)
    host.forget_prompt_cache().await;
    let mut out = Collect::default();
    let messages = vec![cluaiz_core::ChatMessage::new("user", prompt.text.clone())];
    let (_, report) = session.generate(messages, None, &mut out).await?;
    let timings = report.timings.unwrap_or_default();
    let mtp = host.last_context().await.and_then(|c| c.mtp).unwrap_or_default();
    Ok(Answer {
        text: out.0,
        tokens: timings.predicted_n,
        generation_tps: timings.predicted_per_second,
        prompt_tps: timings.prompt_per_second,
        drafted: mtp.drafted,
        accepted: mtp.accepted,
    })
}

pub async fn execute(args: BenchArgs, verbose: bool) -> Result<()> {
    if args.load.mtp.is_some() {
        return Err(eyre!("--mtp-compare measures MTP off and on itself; leave out --mtp <value>"));
    }
    let prompts: Vec<Prompt> = match &args.prompts {
        Some(file) => serde_json::from_str(&std::fs::read_to_string(file)?)
            .map_err(|e| eyre!("{}: {} (a JSON list of {{\"name\", \"text\", \"max_tokens\"}})", file.display(), e))?,
        None => serde_json::from_str(DEFAULT_PROMPTS)?,
    };
    if prompts.is_empty() {
        return Err(eyre!("no prompts to answer"));
    }
    let (path, id) = crate::cli::models::resolve_installed(&args.model)?;
    let host = Arc::new(cluaiz_runtime::EngineHost::new());
    let reps = args.repetitions.max(1);

    // The planner's placement without MTP, then both runs pinned to it, so MTP is the only change
    let mut base = ChatOptions { load: args.load.clone(), ..Default::default() };
    base.load.mtp = Some("off".into());
    let plan = {
        let spin = crate::ui::spinner::Spinner::start("Planning…");
        let plan = host.ensure_loaded(&id, &path, &base.settings()?).await.map_err(|e| eyre!("{}", e))?;
        drop(spin);
        plan
    };
    let geometry = cluaiz_models::GgufProber::probe_geometry(&path).map_err(|e| eyre!("{}", e))?;

    let mut pairs: Vec<Pair> = prompts.iter().map(|p| Pair { prompt: p.name.clone(), off: Vec::new(), on: Vec::new() }).collect();
    for mtp in [false, true] {
        let mut options = ChatOptions {
            load: pinned(&args.load, &plan, mtp),
            local: true,
            think: Some(FeatureState::Off),
            ..Default::default()
        };
        options.samplers.temp = Some(0.0);
        let label = if mtp { "MTP on" } else { "MTP off" };
        let mut session = match Session::open(host.clone(), path.clone(), id.clone(), options.clone()).await {
            Ok(session) => session,
            Err(e) if mtp => {
                eprintln!("MTP on did not load on this placement: {e}");
                break;
            }
            Err(e) => return Err(e),
        };
        if verbose {
            for line in session.plan.summary().lines() {
                eprintln!("  {line}");
            }
        }
        for (prompt, pair) in prompts.iter().zip(pairs.iter_mut()) {
            session.options.max_tokens = Some(prompt.max_tokens);
            eprintln!("{label}: {} ({} runs after one warm-up)…", prompt.name, reps);
            answer(&session, &host, prompt).await?;
            for _ in 0..reps {
                let run = answer(&session, &host, prompt).await?;
                if mtp { pair.on.push(run) } else { pair.off.push(run) }
            }
        }
    }

    let signature = speed::Signature {
        tier: plan.selected_tier,
        n_gpu_layers: plan.n_gpu_layers,
        n_cpu_moe: plan.n_cpu_moe,
        kv_offload: plan.kv_offload,
        moe_cache: plan.moe_cache_bytes > 0,
    };
    let settings = base.settings()?;
    let all_off = mean(pairs.iter().flat_map(|p| p.off.iter().map(|a| a.generation_tps)));
    let all_on = mean(pairs.iter().flat_map(|p| p.on.iter().map(|a| a.generation_tps)));
    speed::record(speed::Test::Text, &geometry, &settings, plan.context_window, plan.kv_cache_type, &signature, all_off);
    if all_on > 0.0 {
        speed::record(speed::Test::TextMtp, &geometry, &settings, plan.context_window, plan.kv_cache_type, &signature, all_on);
    }
    cluaiz_runtime::vram_use::save();

    let rows: Vec<serde_json::Value> = pairs
        .iter()
        .map(|p| {
            let off = mean(p.off.iter().map(|a| a.generation_tps));
            let on = mean(p.on.iter().map(|a| a.generation_tps));
            let drafted: u64 = p.on.iter().map(|a| a.drafted).sum();
            let accepted: u64 = p.on.iter().map(|a| a.accepted).sum();
            let same = !p.on.is_empty() && p.off.first().map(|a| &a.text) == p.on.first().map(|a| &a.text);
            serde_json::json!({
                "prompt": p.prompt,
                "tokens": p.off.first().map_or(0, |a| a.tokens),
                "generation_tps_off": off,
                "generation_tps_on": on,
                "gain": if off > 0.0 && on > 0.0 { on / off } else { 0.0 },
                "prompt_tps_off": mean(p.off.iter().map(|a| a.prompt_tps)),
                "prompt_tps_on": mean(p.on.iter().map(|a| a.prompt_tps)),
                "drafted": drafted,
                "accepted": accepted,
                "kept": if drafted > 0 { accepted as f64 / drafted as f64 } else { 0.0 },
                "same_text": same,
            })
        })
        .collect();

    match args.output {
        Format::Json => println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "model_id": id,
            "placement": plan.placement_label(),
            "n_gpu_layers": plan.n_gpu_layers,
            "n_cpu_moe": plan.n_cpu_moe,
            "n_ctx": plan.context_window,
            "repetitions": reps,
            "prompts": rows,
        }))?),
        Format::Md => {
            println!("{} | {} | ngl {} | n_cpu_moe {} | n_ctx {} | greedy, thinking off, {} runs each", id, plan.placement_label(), plan.n_gpu_layers, plan.n_cpu_moe, plan.context_window, reps);
            println!();
            println!("| prompt | tokens | t/s off | t/s on | gain | kept | prompt t/s off | prompt t/s on | same text |");
            println!("| ------ | -----: | ------: | -----: | ---: | ---: | -------------: | ------------: | :-------: |");
            for r in &rows {
                println!(
                    "| {} | {} | {:.2} | {:.2} | {:.2}x | {:.0}% | {:.1} | {:.1} | {} |",
                    r["prompt"].as_str().unwrap_or_default(),
                    r["tokens"],
                    r["generation_tps_off"].as_f64().unwrap_or(0.0),
                    r["generation_tps_on"].as_f64().unwrap_or(0.0),
                    r["gain"].as_f64().unwrap_or(0.0),
                    r["kept"].as_f64().unwrap_or(0.0) * 100.0,
                    r["prompt_tps_off"].as_f64().unwrap_or(0.0),
                    r["prompt_tps_on"].as_f64().unwrap_or(0.0),
                    if r["same_text"].as_bool().unwrap_or(false) { "yes" } else { "no" },
                );
            }
        }
    }
    Ok(())
}
