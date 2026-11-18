//! `cluaiz show <model>`: what the GGUF file says about the model, this machine now, the settings
//! that change the plan, and the plan it would get (nothing is loaded). `-v` adds the full planner
//! banner, every metadata key and every tensor.

use cluaiz_core::{bytes_to_gb, bytes_to_gib, group_digits, Modality};
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

fn heading(title: &str) {
    println!("{}", title.bold());
}

fn row(key: &str, value: impl std::fmt::Display) {
    println!("  {:<14}{}", key, value);
}

/// Lines from the plan report, with the section titles in bold
fn print_lines(lines: impl IntoIterator<Item = impl AsRef<str>>) {
    for line in lines {
        let line = line.as_ref();
        let is_title = !line.is_empty() && !line.starts_with(' ');
        if is_title { heading(line) } else { println!("{}", line) }
    }
}

fn yes_no(on: bool) -> &'static str {
    if on { "yes" } else { "no" }
}

pub async fn execute(args: ShowArgs, verbose: bool) -> Result<()> {
    let (path, id) = crate::cli::models::resolve_installed(&args.model)?;
    let is_gguf = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    if !is_gguf {
        heading("MODEL");
        row("ID", &id);
        row("File", path.display());
        println!("\n  Details and plans are shown for GGUF models only.");
        return Ok(());
    }

    let spin = crate::ui::spinner::Spinner::start("Reading model file…");
    let header = GgufProber::read_header(&path).map_err(|e| eyre!("{}: {}", path.display(), e))?;
    let geometry = GgufProber::geometry_from_header(&path, &header).map_err(|e| eyre!("{}: {}", path.display(), e))?;
    drop(spin);
    let meta = |k: &str| header.metadata.get(k).cloned();

    // MODEL
    heading("MODEL");
    row("ID", &id);
    if let Some(name) = meta("general.name") {
        row("Name", name);
    }
    if let Some(size) = meta("general.size_label") {
        row("Parameters", size);
    }
    let w = &geometry.weights;
    if geometry.is_moe() {
        row("Type", format!("MoE ({}): {} experts, {} active per token", geometry.architecture,
            geometry.expert_count.unwrap_or(0), geometry.expert_used_count.unwrap_or(0)));
    } else {
        row("Type", format!("dense ({}): every token reads the whole model", geometry.architecture));
    }
    row("Layers", format!("{} blocks | embedding {} | heads {} (KV {})", geometry.block_count, geometry.embedding_length,
        geometry.head_count, geometry.head_count_kv));
    let window = geometry.sliding_window.map(|w| format!(" | sliding window {}", group_digits(w as u64))).unwrap_or_default();
    row("Context max", format!("{} tokens{}", group_digits(geometry.context_length as u64), window));
    row("File", format!("{:.2} GB (as Hugging Face counts it) | {}", bytes_to_gb(geometry.file_size_bytes), path.display()));
    if geometry.is_moe() {
        row("Parts", format!("attention and shared {:.2} GB | experts {:.2} GB ({:.2} GB per block)",
            bytes_to_gib(w.full_gpu() - w.experts_total()), bytes_to_gib(w.experts_total()),
            bytes_to_gib(w.experts_total()) / geometry.block_count.max(1) as f64));
    }

    // Supports: inputs as the model declares them; thinking tags as llama.cpp's template engine reads them
    let template = meta("tokenizer.chat_template");
    let thinking = if template.is_some() {
        match cluaiz_runtime::EngineHost::new().get_thinking_tags(template.as_deref()).await {
            (Some(start), Some(end)) => format!("yes ({} ... {})", start, end),
            (Some(start), None) => format!("yes ({})", start),
            _ => "no".to_string(),
        }
    } else {
        "no".to_string()
    };
    let dir = path.parent().unwrap_or(&path);
    let inputs = cluaiz_models::capabilities::PackageCapabilities::from_header(&path, dir, &header).supported_inputs();
    row("Supports", format!("text {} | image {} | audio {} | tools {} | thinking {}",
        yes_no(true), yes_no(inputs.contains(&Modality::Image)), yes_no(inputs.contains(&Modality::Audio)),
        yes_no(cluaiz_api::handlers::chat::template_renders_tools(template.as_deref())), thinking));
    println!();

    let mut settings = cluaiz_core::Settings::current();
    args.load.apply(&mut settings.optimization).map_err(|e| eyre!(e))?;
    // What models really took on this GPU, as a load uses it
    cluaiz_runtime::vram_use::load();
    let hw = cluaiz_hw::HardwareProber::with_live_memory();
    let plan = Planner::create_plan(&id, &geometry, &hw, &settings);

    print_lines(cluaiz_plan::report::machine(&hw));
    println!();
    print_lines(cluaiz_plan::report::user_settings(&settings));
    println!();
    print_lines(plan.report(&geometry, &hw, &settings).lines());
    println!();
    println!("{}", "Memory in GB of 1024^3 bytes, as the OS reports RAM and VRAM; file size as Hugging Face counts it".dimmed());

    if verbose {
        println!("{}", plan.format_telemetry_banner(&geometry, &hw, &settings));
        let (metadata, tensors, _) = GgufProber::probe(&path).map_err(|e| eyre!("{}: {}", path.display(), e))?;
        heading(&format!("METADATA ({} keys)", metadata.len()));
        let mut keys: Vec<_> = metadata.iter().collect();
        keys.sort();
        for (k, v) in keys {
            let shown: String = if v.chars().count() > 120 { format!("{}...", v.chars().take(120).collect::<String>()) } else { v.clone() };
            println!("  {:<40}{}", k, shown.replace('\n', "\\n"));
        }
        heading(&format!("TENSORS ({})", tensors.len()));
        let mut names: Vec<_> = tensors.iter().collect();
        names.sort();
        for (name, dims) in names {
            println!("  {:<40}{:?}", name, dims);
        }
    }
    Ok(())
}
