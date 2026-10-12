//! `cluaiz show <model>`: what the GGUF file says about the model, and the plan it would get on this
//! machine right now (nothing is loaded). `-v` adds every metadata key and the full planner banner.

use cluaiz_models::GgufProber;
use cluaiz_plan::Planner;
use color_eyre::{eyre::eyre, Result};
use colored::Colorize;

use crate::cli::args::LoadArgs;

#[derive(clap::Args)]
pub struct ShowArgs {
    /// Installed model id or path to a .gguf file
    pub model: String,
    #[command(flatten)]
    pub load: LoadArgs,
}

fn section(title: &str) {
    println!("  {}", title.bold());
}

fn row(key: &str, value: impl std::fmt::Display) {
    println!("    {:<18}{}", key, value);
}

pub async fn execute(args: ShowArgs, verbose: bool) -> Result<()> {
    let (path, id) = crate::cli::models::resolve_installed(&args.model)?;
    let is_gguf = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gguf")).unwrap_or(false);
    if !is_gguf {
        section("Model");
        row("id", &id);
        row("file", path.display());
        println!("\n  Details and plans are shown for GGUF models only.");
        return Ok(());
    }

    let _spin = crate::ui::spinner::Spinner::start("Reading model file…");
    let geometry = GgufProber::probe_geometry(&path).map_err(|e| eyre!("{}: {}", path.display(), e))?;
    let (metadata, tensors, _) = GgufProber::probe(&path).map_err(|e| eyre!("{}: {}", path.display(), e))?;
    drop(_spin);
    let meta = |k: &str| metadata.get(k).cloned();

    section("Model");
    row("id", &id);
    row("file", format!("{} ({:.2} GiB)", path.display(), geometry.file_size_bytes as f64 / cluaiz_core::GIB_F64));
    row("architecture", &geometry.architecture);
    if let Some(name) = meta("general.name") {
        row("name", name);
    }
    if let Some(size) = meta("general.size_label") {
        row("parameters", size);
    }
    row("context length", geometry.context_length);
    row("layers", geometry.block_count);
    row("embedding", geometry.embedding_length);
    row("attention heads", format!("{} (KV {})", geometry.head_count, geometry.head_count_kv));
    if geometry.is_moe() {
        row("experts", format!("{} ({} active)", geometry.expert_count.unwrap_or(0), geometry.expert_used_count.unwrap_or(0)));
    }
    if let Some(window) = geometry.sliding_window {
        row("sliding window", window);
    }

    // Thinking tags as llama.cpp's template engine reads them (the engine uses the same call)
    let template = meta("tokenizer.chat_template");
    section("Capabilities");
    row("chat template", if template.is_some() { "yes" } else { "no" });
    if template.is_some() {
        let host = cluaiz_runtime::EngineHost::new();
        let thinking = match host.get_thinking_tags(template.as_deref()).await {
            (Some(start), Some(end)) => format!("yes ({} ... {})", start, end),
            (Some(start), None) => format!("yes ({})", start),
            _ => "no".to_string(),
        };
        row("thinking", thinking);
    }

    let mut settings = cluaiz_core::Settings::current();
    args.load.apply(&mut settings.optimization).map_err(|e| eyre!(e))?;
    let hw = cluaiz_hw::HardwareProber::with_live_memory();
    let plan = Planner::create_plan(&id, &geometry, &hw, &settings);
    section("Plan on this machine now");
    for line in plan.summary().lines() {
        println!("    {}", line);
    }

    if verbose {
        println!("{}", plan.format_telemetry_banner(&geometry, &hw, &settings));
        section(&format!("Metadata ({} keys)", metadata.len()));
        let mut keys: Vec<_> = metadata.iter().collect();
        keys.sort();
        for (k, v) in keys {
            let shown: String = if v.chars().count() > 120 { format!("{}...", v.chars().take(120).collect::<String>()) } else { v.clone() };
            row(k, shown.replace('\n', "\\n"));
        }
        section(&format!("Tensors ({})", tensors.len()));
        let mut names: Vec<_> = tensors.iter().collect();
        names.sort();
        for (name, dims) in names {
            println!("    {:<40}{:?}", name, dims);
        }
    }
    Ok(())
}
