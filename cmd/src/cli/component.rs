use color_eyre::Result;
use colored::Colorize;
use crate::ComponentCommand;

pub async fn execute(component_type: &str, command: ComponentCommand) -> Result<()> {
    match command {
        ComponentCommand::Install { component_name } => {
            install_component(component_type, &component_name).await?;
        }
        ComponentCommand::List => {
            list_components(component_type).await?;
        }
        ComponentCommand::Cache { command } => {
            handle_cache_command(component_type, command).await?;
        }
        ComponentCommand::Remove { component_name } => {
            println!("  {} [Cluaiz {}] Removing: {}", "🗑️".cyan(), component_type.to_uppercase(), component_name.bold());
            if let Err(e) = engines::neural_foundry::registry::hub_installer::HubInstaller::remove_component(component_type, &component_name).await {
                println!("Error removing {}: {}", component_type, e);
            } else {
                println!("  {} Successfully removed {}", "✅".green(), component_name.bold());
            }
        }
        ComponentCommand::Start { component_name } => {
            println!("  {} [Cluaiz {}] Starting daemon for: {}", "🚀".cyan(), component_type.to_uppercase(), component_name.bold());
            // TODO: Start daemon logic
        }
        ComponentCommand::Link { plugin_name, skill_name } => {
            println!("  {} [Cluaiz Plugin] Linking {} to {}", "🔗".cyan(), plugin_name.bold(), skill_name.bold());
            // TODO: Link logic
        }
    }
    Ok(())
}

async fn handle_cache_command(component_type: &str, command: crate::ComponentCacheCommand) -> Result<()> {
    match command {
        crate::ComponentCacheCommand::Ls => {
            println!("\n  {} [Cluaiz Dual-Cache] Scanning Global {} Memory...", "🧠".cyan(), component_type.to_uppercase());
            match engines::neural_foundry::registry::hub_installer::HubInstaller::list_component_cache(component_type) {
                Ok(report) => println!("{}", report),
                Err(e) => println!("Error listing cache: {}", e),
            }
        }
        crate::ComponentCacheCommand::Clear { component_id, all, force } => {
            println!("\n  {} [Cluaiz Dual-Cache] Initiating Global Wipe for {}...", "🧹".yellow(), component_type.to_uppercase());
            match engines::neural_foundry::registry::hub_installer::HubInstaller::clear_component_cache(component_type, component_id, all, force) {
                Ok(wiped) => println!("\n    Successfully wiped {} caches.\n", wiped),
                Err(e) => println!("Error clearing cache: {}", e),
            }
        }
    }
    Ok(())
}

async fn install_component(component_type: &str, component_name: &str) -> Result<()> {
    if let Err(e) = engines::neural_foundry::registry::hub_installer::HubInstaller::install_component(component_type, component_name).await {
        println!("Error installing {}: {}", component_type, e);
    }
    Ok(())
}

async fn list_components(component_type: &str) -> Result<()> {
    println!("\n  {} [Cluaiz] Installed Sovereign {}:", "📦".cyan(), component_type.to_uppercase());
    match engines::neural_foundry::registry::hub_installer::HubInstaller::list_installed_components(component_type) {
        Ok(components) => {
            if components.is_empty() {
                println!("    No {} installed yet. Use `cluaiz {} install <name>`.", component_type, component_type);
            } else {
                for name in components {
                    println!("    {} {}", "🔹".blue(), name.bold());
                }
            }
        }
        Err(_) => {
            println!("    No {} installed yet.", component_type);
        }
    }
    println!();
    Ok(())
}
