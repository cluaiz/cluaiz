use color_eyre::Result;
use colored::Colorize;
use crate::PluginCommand;
use engines::neural_foundry::registry::registry_index::MasterRegistry;

pub async fn execute(command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::Install { plugin_name } => {
            println!("  {} [cluaiz Plugins] Installing plugin: {}", "🔌".cyan(), plugin_name.bold());
            match engines::neural_foundry::registry::plugin_manager::PluginManager::install_plugin(&plugin_name).await {
                Ok(_) => println!("  {} Plugin '{}' installed successfully.", "✅".green(), plugin_name.bold()),
                Err(e) => println!("  {} Failed to install plugin: {}", "❌".red(), e),
            }
        }

        PluginCommand::List => {
            println!("\n  {} [cluaiz Plugins] Registered Plugins:", "🔌".cyan());
            match MasterRegistry::load() {
                Ok(registry) => {
                    if registry.plugins.is_empty() {
                        println!("    (No plugins registered)");
                    } else {
                        for (name, entry) in &registry.plugins {
                            let status = if entry.enabled { "✅".green() } else { "❌".red() };
                            let strategy = format!("{:?}", entry.load_strategy);
                            println!("    {} {} [{}] — {}", status, name.bold(), strategy.cyan(), entry.domain);
                        }
                    }
                }
                Err(_) => println!("    (No registry found — no plugins installed)"),
            }
        }

        PluginCommand::Remove { plugin_name } => {
            println!("  {} [cluaiz Plugins] Removing plugin: {}", "🔌".cyan(), plugin_name.bold());
            match engines::neural_foundry::registry::plugin_manager::PluginManager::remove_plugin(&plugin_name).await {
                Ok(_) => println!("  {} Plugin '{}' removed.", "✅".green(), plugin_name.bold()),
                Err(e) => println!("  {} Failed to remove plugin: {}", "⚠️".yellow(), e),
            }
        }

        PluginCommand::Cache { command } => {
            match command {
                crate::PluginCacheCommand::Ls => {
                    println!("\n  {} [cluaiz Plugins] Plugin Cache Status:", "🔌".cyan());
                    match MasterRegistry::load() {
                        Ok(registry) => {
                            for (name, entry) in &registry.plugins {
                                let global_dir = cluaiz_shared::environment::EnvironmentManager::current().global_dir;
                                let cache_path = global_dir.join(&entry.domain).join(".cache");
                                let cache_exists = if cache_path.exists() { "📦 cached".yellow() } else { "○ empty".dimmed() };
                                println!("    {} {} — {}", "🔌".cyan(), name.bold(), cache_exists);
                            }
                        }
                        Err(_) => println!("    (No registry found)"),
                    }
                }
                crate::PluginCacheCommand::Clear { plugin_name, all } => {
                    println!("  {} [cluaiz Plugins] Clearing plugin cache...", "🔌".cyan());
                    let target = if all { None } else { plugin_name.as_deref() };
                    match engines::neural_foundry::registry::plugin_manager::PluginManager::clear_plugin_cache(target).await {
                        Ok(wiped) => println!("  {} Successfully wiped {} plugin cache(s).", "✅".green(), wiped),
                        Err(e) => println!("  {} Error clearing plugin cache: {}", "❌".red(), e),
                    }
                }
            }
        }

        PluginCommand::Link { plugin_name, skill_name } => {
            println!("  {} [cluaiz Plugins] Linking plugin '{}' to skill '{}'", "🔌".cyan(), plugin_name.bold(), skill_name.bold());
            // TODO: Update skill's manifest to declare this plugin dependency
        }
    }
    Ok(())
}
