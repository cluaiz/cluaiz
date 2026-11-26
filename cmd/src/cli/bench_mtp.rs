//! `cluaiz bench <model> --mtp-compare`: MTP speculative decoding off and on, on real text, on the
//! same placement. llama-bench's `tg` test generates from random tokens, where the model's own guesses
//! mean nothing, so MTP is measured by answering prompts through the normal chat path (`Session`),
//! greedy and without thinking: generation and prompt speed per answer, the share of guesses kept,
//! the VRAM each load took, and whether the text is the same with and without MTP.
//!
//! MTP off, on and Auto (the load rules and the per-answer guard) alternate in every round, each a
//! fresh load: a laptop GPU
//! changes its clock between minutes (the same plain run gave 55 and 72 t/s on the laptop, 2026-11-19), so measuring all
//! "off" runs first and all "on" runs later would put that drift into the gain. With `--placements`
//! every placement the planner can use is measured this way (full GPU, split, CPU only, MoE split,
//! experts read from disk), which is what the Auto rules need: one model can gain in one placement
//! and lose in another.
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

/// One answer: speeds as the Engine reports them (with MTP the stream times prompt and generation
/// itself, see `NativeLlama::mtp_timings`)
#[derive(Clone, Default)]
struct Answer {
    text: String,
    tokens: i32,
    generation_tps: f64,
    prompt_tps: f64,
    drafted: u64,
    accepted: u64,
    /// The Auto guard stopped guessing in this answer
    stopped: bool,
}

/// A placement to measure: the load flags that give it
#[derive(Clone, PartialEq)]
struct Placement {
    n_gpu_layers: i32,
    n_cpu_moe: u32,
    kv_offload: bool,
}

/// Results of one placement
struct Measured {
    plan: ExecutionPlan,
    /// VRAM the last load took, off and on (load history; None = not measured)
    vram_off: Option<f64>,
    vram_on: Option<f64>,
    /// Why MTP on did not load (None = it did)
    on_failed: Option<String>,
    /// [prompt][run]
    off: Vec<Vec<Answer>>,
    on: Vec<Vec<Answer>>,
    /// MTP Auto: the load rules decide, the guard may stop guessing per answer
    auto: Vec<Vec<Answer>>,
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

/// The load flags that give exactly `placement` with `plan`'s context and KV type, MTP on or off
fn pinned(base: &LoadArgs, plan: &ExecutionPlan, placement: &Placement, mtp: &str) -> LoadArgs {
    let mut load = base.clone();
    load.n_gpu_layers = Some(GpuLayers::Count(placement.n_gpu_layers));
    load.n_cpu_moe = Some(placement.n_cpu_moe);
    load.no_kv_offload = !placement.kv_offload;
    load.ctx_size = Some(plan.context_window as i64);
    load.cache_type_k = cache_type(plan.kv_cache_type);
    load.cache_type_v = load.cache_type_k.clone();
    load.mtp = Some(mtp.into());
    load
}

/// VRAM the newest load of `model` took, from the load history
fn last_vram_taken(model: &str) -> Option<f64> {
    cluaiz_runtime::load_history::read(Some(model), 0, usize::MAX).records.last().and_then(|r| r.vram_taken_gb)
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
        stopped: mtp.stopped,
    })
}

/// Loads `placement` with MTP off or on and answers every prompt once (after one warm-up)
async fn round(
    host: &Arc<cluaiz_runtime::EngineHost>,
    args: &BenchArgs,
    model: (&std::path::Path, &str),
    plan: &ExecutionPlan,
    placement: &Placement,
    mtp: &str,
    prompts: &[Prompt],
    verbose: bool,
) -> Result<(ExecutionPlan, Option<f64>, Vec<Answer>)> {
    let (path, id) = model;
    let mut options = ChatOptions {
        load: pinned(&args.load, plan, placement, mtp),
        local: true,
        think: Some(FeatureState::Off),
        ..Default::default()
    };
    options.samplers.temp = Some(args.temperature.unwrap_or(0.0));
    let mut session = Session::open(host.clone(), path.to_path_buf(), id.to_string(), options).await?;
    let vram = last_vram_taken(id);
    if verbose {
        for line in session.plan.summary().lines() {
            eprintln!("  {line}");
        }
    }
    let mut answers = Vec::new();
    for (i, prompt) in prompts.iter().enumerate() {
        session.options.max_tokens = Some(prompt.max_tokens);
        if i == 0 {
            // the first answer after a load is slower (warm-up of kernels and caches)
            answer(&session, host, prompt).await?;
        }
        answers.push(answer(&session, host, prompt).await?);
    }
    Ok((session.plan.clone(), vram, answers))
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

    // The planner's placement without MTP; every run is pinned to a placement, so MTP is the only change
    let mut base = ChatOptions { load: args.load.clone(), ..Default::default() };
    base.load.mtp = Some("off".into());
    if args.placements && base.settings()?.optimization.is_manual_placement() {
        return Err(eyre!("--placements measures the planner's placements; leave out --n-gpu-layers, --n-cpu-moe and --no-kv-offload"));
    }
    let plan = {
        let spin = crate::ui::spinner::Spinner::start("Planning…");
        let plan = host.ensure_loaded(&id, &path, &base.settings()?).await.map_err(|e| eyre!("{}", e))?;
        drop(spin);
        plan
    };
    let geometry = cluaiz_models::GgufProber::probe_geometry(&path).map_err(|e| eyre!("{}", e))?;

    let mut placements = vec![Placement { n_gpu_layers: plan.n_gpu_layers, n_cpu_moe: plan.n_cpu_moe, kv_offload: plan.kv_offload }];
    if args.placements {
        for c in plan.candidates.iter().filter(|c| c.feasible) {
            let p = Placement { n_gpu_layers: c.n_gpu_layers, n_cpu_moe: c.n_cpu_moe, kv_offload: c.kv_offload };
            if !placements.contains(&p) {
                placements.push(p);
            }
        }
    }

    let model = (path.as_path(), id.as_str());
    let mut results: Vec<Measured> = Vec::new();
    for (n, placement) in placements.iter().enumerate() {
        let mut measured = Measured {
            plan: plan.clone(),
            vram_off: None,
            vram_on: None,
            on_failed: None,
            off: vec![Vec::new(); prompts.len()],
            on: vec![Vec::new(); prompts.len()],
            auto: vec![Vec::new(); prompts.len()],
        };
        for r in 0..reps {
            for mtp in ["off", "on", "auto"] {
                if mtp != "off" && measured.on_failed.is_some() {
                    continue;
                }
                eprintln!(
                    "placement {}/{} (ngl {}, n_cpu_moe {}), round {}/{}, MTP {}…",
                    n + 1, placements.len(), placement.n_gpu_layers, placement.n_cpu_moe, r + 1, reps,
                    mtp
                );
                match round(&host, &args, model, &plan, placement, mtp, &prompts, verbose).await {
                    Ok((loaded, vram, answers)) => {
                        let slot = match mtp {
                            "on" => &mut measured.on,
                            "auto" => &mut measured.auto,
                            _ => &mut measured.off,
                        };
                        for (runs, a) in slot.iter_mut().zip(answers) {
                            runs.push(a);
                        }
                        match mtp {
                            "on" => measured.vram_on = vram.or(measured.vram_on),
                            "off" => {
                                measured.vram_off = vram.or(measured.vram_off);
                                measured.plan = loaded;
                            }
                            _ => {}
                        }
                    }
                    Err(e) if mtp != "off" => {
                        eprintln!("MTP on did not load on this placement: {e}");
                        measured.on_failed = Some(e.to_string());
                    }
                    Err(e) if n > 0 => {
                        eprintln!("this placement did not load: {e}");
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        if !measured.off[0].is_empty() {
            results.push(measured);
        }
    }

    // Kept with the placement speeds, so the Auto rules can read a measured result
    let settings = base.settings()?;
    for m in &results {
        let p = &m.plan;
        let signature = speed::Signature {
            tier: p.selected_tier,
            n_gpu_layers: p.n_gpu_layers,
            n_cpu_moe: p.n_cpu_moe,
            kv_offload: p.kv_offload,
            moe_cache: p.moe_cache_bytes > 0,
        };
        let off = mean(m.off.iter().flatten().map(|a| a.generation_tps));
        let on = mean(m.on.iter().flatten().map(|a| a.generation_tps));
        speed::record(speed::Test::Text, &geometry, &settings, p.context_window, p.kv_cache_type, &signature, off);
        if on > 0.0 {
            speed::record(speed::Test::TextMtp, &geometry, &settings, p.context_window, p.kv_cache_type, &signature, on);
        }
    }
    cluaiz_runtime::vram_use::save();

    let report: Vec<serde_json::Value> = results
        .iter()
        .map(|m| {
            let rows: Vec<serde_json::Value> = prompts
                .iter()
                .enumerate()
                .map(|(i, prompt)| {
                    let (off, on, auto) = (&m.off[i], &m.on[i], &m.auto[i]);
                    let off_tps = mean(off.iter().map(|a| a.generation_tps));
                    let on_tps = mean(on.iter().map(|a| a.generation_tps));
                    let auto_tps = mean(auto.iter().map(|a| a.generation_tps));
                    let drafted: u64 = on.iter().map(|a| a.drafted).sum();
                    let accepted: u64 = on.iter().map(|a| a.accepted).sum();
                    // gain per round (off and on of the same round were measured minutes apart at most)
                    let gains: Vec<f64> = off
                        .iter()
                        .zip(on.iter())
                        .filter(|(a, b)| a.generation_tps > 0.0 && b.generation_tps > 0.0)
                        .map(|(a, b)| b.generation_tps / a.generation_tps)
                        .collect();
                    serde_json::json!({
                        "prompt": prompt.name,
                        "tokens": off.first().map_or(0, |a| a.tokens),
                        "generation_tps_off": off_tps,
                        "generation_tps_on": on_tps,
                        "gain": if off_tps > 0.0 && on_tps > 0.0 { on_tps / off_tps } else { 0.0 },
                        "generation_tps_auto": auto_tps,
                        "gain_auto": if off_tps > 0.0 && auto_tps > 0.0 { auto_tps / off_tps } else { 0.0 },
                        // Auto answers that guessed at all (0 = the load rules turned MTP off) and
                        // those where the guard stopped guessing
                        "auto_drafted_runs": auto.iter().filter(|a| a.drafted > 0).count(),
                        "auto_stopped_runs": auto.iter().filter(|a| a.stopped).count(),
                        "auto_runs": auto.len(),
                        "gain_min": if gains.is_empty() { 0.0 } else { gains.iter().cloned().fold(f64::INFINITY, f64::min) },
                        "gain_max": gains.iter().cloned().fold(0.0, f64::max),
                        "prompt_tps_off": mean(off.iter().map(|a| a.prompt_tps)),
                        "prompt_tps_on": mean(on.iter().map(|a| a.prompt_tps)),
                        "drafted": drafted,
                        "accepted": accepted,
                        "kept": if drafted > 0 { accepted as f64 / drafted as f64 } else { 0.0 },
                        "same_text": !on.is_empty() && off.first().map(|a| &a.text) == on.first().map(|a| &a.text),
                    })
                })
                .collect();
            serde_json::json!({
                "placement": m.plan.placement_label(),
                "n_gpu_layers": m.plan.n_gpu_layers,
                "n_cpu_moe": m.plan.n_cpu_moe,
                "kv_offload": m.plan.kv_offload,
                "n_ctx": m.plan.context_window,
                "disk_stream_gb": m.plan.disk_stream_gb,
                "vram_taken_gb_off": m.vram_off,
                "vram_taken_gb_on": m.vram_on,
                "mtp_on_failed": m.on_failed,
                "prompts": rows,
            })
        })
        .collect();

    match args.output {
        Format::Json => println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "model_id": id,
            "temperature": args.temperature.unwrap_or(0.0),
            "rounds": reps,
            "placements": report,
        }))?),
        Format::Md => {
            let sampling = match args.temperature {
                Some(t) if t > 0.0 => format!("temperature {t}"),
                _ => "greedy".to_string(),
            };
            println!("{} | {}, thinking off, {} rounds of MTP off, on and auto (fresh load each)", id, sampling, reps);
            let gb = |v: &serde_json::Value| v.as_f64().map_or("?".to_string(), |g| format!("{g:.2} GB"));
            for p in &report {
                println!();
                println!(
                    "{} | ngl {} | n_cpu_moe {} | n_ctx {} | from disk {:.1} GB | VRAM taken: off {}, on {}",
                    p["placement"].as_str().unwrap_or_default(),
                    p["n_gpu_layers"],
                    p["n_cpu_moe"],
                    p["n_ctx"],
                    p["disk_stream_gb"].as_f64().unwrap_or(0.0),
                    gb(&p["vram_taken_gb_off"]),
                    gb(&p["vram_taken_gb_on"]),
                );
                if let Some(e) = p["mtp_on_failed"].as_str() {
                    println!("MTP on did not load: {e}");
                }
                println!();
                println!("| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |");
                println!("| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |");
                for r in p["prompts"].as_array().into_iter().flatten() {
                    println!(
                        "| {} | {} | {:.2} | {:.2} | {:.2}x | {:.2}-{:.2} | {:.0}% | {:.2} | {:.2}x | {}/{} of {} | {:.1} | {:.1} | {} |",
                        r["prompt"].as_str().unwrap_or_default(),
                        r["tokens"],
                        r["generation_tps_off"].as_f64().unwrap_or(0.0),
                        r["generation_tps_on"].as_f64().unwrap_or(0.0),
                        r["gain"].as_f64().unwrap_or(0.0),
                        r["gain_min"].as_f64().unwrap_or(0.0),
                        r["gain_max"].as_f64().unwrap_or(0.0),
                        r["kept"].as_f64().unwrap_or(0.0) * 100.0,
                        r["generation_tps_auto"].as_f64().unwrap_or(0.0),
                        r["gain_auto"].as_f64().unwrap_or(0.0),
                        r["auto_drafted_runs"],
                        r["auto_stopped_runs"],
                        r["auto_runs"],
                        r["prompt_tps_off"].as_f64().unwrap_or(0.0),
                        r["prompt_tps_on"].as_f64().unwrap_or(0.0),
                        if r["same_text"].as_bool().unwrap_or(false) { "yes" } else { "no" },
                    );
                }
            }
        }
    }
    Ok(())
}
