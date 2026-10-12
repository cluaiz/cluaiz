//! `cluaiz rm <id>`: delete an installed model (its folder and its registry entry).

use color_eyre::{eyre::eyre, Result};
use colored::Colorize;

pub async fn execute(model_id: &str) -> Result<()> {
    let registry = cluaiz_models::registry::InstalledStateRegistry::load();
    // Deleting needs the exact id (case aside); a partial name only gets suggestions
    let matches = registry.find(model_id);
    let id = match matches.iter().find(|e| e.id.eq_ignore_ascii_case(model_id.trim())) {
        Some(entry) => entry.id.clone(),
        None if matches.is_empty() => return Err(eyre!("model '{}' is not installed (see `cluaiz list`)", model_id)),
        None => {
            let ids: Vec<_> = matches.iter().map(|e| e.id.as_str()).collect();
            return Err(eyre!("no installed model is called '{}'. Did you mean: {}?", model_id, ids.join(", ")));
        }
    };
    let dir = cluaiz_models::ModelDownloader::uninstall(&id).map_err(|e| eyre!(e))?;
    println!("  {} Removed '{}' ({})", "🗑️".red(), id.bold(), dir.display());
    Ok(())
}
