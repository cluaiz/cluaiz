//! `cluaiz list`: installed models (the registry `run`, `show` and the API resolve ids from).

use color_eyre::Result;
use colored::Colorize;

pub async fn execute() -> Result<()> {
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    if registry.installed_models.is_empty() {
        println!("No models installed. Download one with `cluaiz pull <id>`.");
        return Ok(());
    }
    let active = crate::cli::models::default_chat_model();
    let mut entries: Vec<_> = registry.installed_models.values().collect();
    entries.sort_by(|a, b| a.category.cmp(&b.category).then(a.id.cmp(&b.id)));

    let id_width = entries.iter().map(|e| e.id.len() + 2).max().unwrap_or(2);
    let header = format!("{:<id_width$}  {:<10}  {:<6}  {:>10}", "ID", "CATEGORY", "FORMAT", "SIZE", id_width = id_width);
    println!("{}", header.bold());
    for e in entries {
        let size = format!("{:.2} GiB", e.size_bytes() as f64 / cluaiz_core::GIB_F64);
        let id = if active.as_deref() == Some(e.id.as_str()) { format!("{} *", e.id) } else { e.id.clone() };
        println!("{:<id_width$}  {:<10}  {:<6}  {:>10}", id, e.category, e.format_type, size, id_width = id_width);
    }
    if active.is_some() {
        eprintln!("{}", "* active chat model (used by `cluaiz run` without a model)".dimmed());
    }
    Ok(())
}
