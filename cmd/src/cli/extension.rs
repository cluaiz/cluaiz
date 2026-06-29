use color_eyre::Result;
use colored::Colorize;
use crate::ExtensionCommand;
use engines::neural_foundry::registry::registry_index::MasterRegistry;

pub async fn execute(command: ExtensionCommand) -> Result<()> {
    match command {
        ExtensionCommand::Install { extension_name } => {
            println!("  {} [cluaiz Extensions] Installing extension: {}", "🧩".cyan(), extension_name.bold());
            match engines::neural_foundry::registry::extension_manager::ExtensionManager::install_extension(&extension_name).await {
                Ok(_) => println!("  {} Extension '{}' installed successfully.", "✅".green(), extension_name.bold()),
                Err(e) => println!("  {} Failed to install extension: {}", "❌".red(), e),
            }
        }

        ExtensionCommand::List => {
            println!("\n  {} [cluaiz Extensions] Registered Extensions:", "🧩".cyan());
            match MasterRegistry::load() {
                Ok(registry) => {
                    if registry.extensions.is_empty() {
                        println!("    (No extensions registered)");
                    } else {
                        for (name, entry) in &registry.extensions {
                            let status = if entry.enabled { "✅".green() } else { "❌".red() };
                            let strategy = format!("{:?}", entry.load_strategy);
                            println!("    {} {} [{}] — {}", status, name.bold(), strategy.cyan(), entry.domain);
                        }
                    }
                }
                Err(_) => println!("    (No registry found — no extensions installed)"),
            }
        }

        ExtensionCommand::Remove { extension_name } => {
            println!("  {} [cluaiz Extensions] Removing extension: {}", "🧩".cyan(), extension_name.bold());
            match engines::neural_foundry::registry::extension_manager::ExtensionManager::remove_extension(&extension_name).await {
                Ok(_) => println!("  {} Extension '{}' removed.", "✅".green(), extension_name.bold()),
                Err(e) => println!("  {} Failed to remove extension: {}", "⚠️".yellow(), e),
            }
        }

        ExtensionCommand::Cache { command } => {
            match command {
                crate::ExtensionCacheCommand::Ls => {
                    println!("\n  {} [cluaiz Extensions] Extension Cache Status:", "🧩".cyan());
                    match MasterRegistry::load() {
                        Ok(registry) => {
                            for (name, entry) in &registry.extensions {
                                let global_dir = cluaiz_shared::environment::EnvironmentManager::current().global_dir;
                                let cache_path = global_dir.join(&entry.domain).join(".cache");
                                let cache_exists = if cache_path.exists() { "📦 cached".yellow() } else { "○ empty".dimmed() };
                                println!("    {} {} — {}", "🧩".cyan(), name.bold(), cache_exists);
                            }
                        }
                        Err(_) => println!("    (No registry found)"),
                    }
                }
                crate::ExtensionCacheCommand::Clear { extension_name, all } => {
                    println!("  {} [cluaiz Extensions] Clearing extension cache...", "🧩".cyan());
                    let target = if all { None } else { extension_name.as_deref() };
                    match engines::neural_foundry::registry::extension_manager::ExtensionManager::clear_extension_cache(target).await {
                        Ok(wiped) => println!("  {} Successfully wiped {} extension cache(s).", "✅".green(), wiped),
                        Err(e) => println!("  {} Error clearing extension cache: {}", "❌".red(), e),
                    }
                }
            }
        }

        ExtensionCommand::Start { extension_name } => {
            println!("  {} [cluaiz Extensions] Starting background daemon for '{}'", "🚀".cyan(), extension_name.bold());
            // TODO: Call ExtensionManager::start when PROCESS sandbox is implemented
        }
    }
    Ok(())
}
