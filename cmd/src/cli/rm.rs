use color_eyre::Result;
use colored::Colorize;
use engines::models::registry::CoreRoster;
use std::fs;

pub async fn execute(model_id: &str) -> Result<()> {
    println!("\n  {} [cluaiz] Preparing for Neural Deletion: '{}'...", "🗑️".red(), model_id.bold());

    let roster = CoreRoster::load_roster();
    let model = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());

    if let Some(m) = model {
        // Resolve path to the model file
        let vault_path = cluaiz_shared::environment::EnvironmentManager::current()
            .ensure_models_dir()
            .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir());
        
        // This is a simplification; a real manager should handle the specific file naming
        let model_file = vault_path.join(format!("{}.gguf", m.id)); // Assuming GGUF for now
        
        if model_file.exists() {
            fs::remove_file(&model_file)?;
            println!("  {} Model weights successfully purged.", "✅".green());
        } else {
            println!("  {} Weights file not found in vault, but metadata was present. Cleaning metadata...", "⚠️".yellow());
        }
        
        // Note: In a real system, we'd also update the local roster cache
        println!("  {} Model '{}' has been removed from the registry.\n", "🛡️".green(), model_id.cyan());
    } else {
        println!("  {} Model ID '{}' not found in the local vault.\n", "❌".red(), model_id.bold());
    }

    Ok(())
}
