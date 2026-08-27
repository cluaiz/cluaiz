use color_eyre::Result;
use colored::Colorize;
use crate::ComponentCommand;
use engines::tools::{ToolHubInstaller, ToolsEngine};

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
            remove_component(component_type, &component_name).await?;
        }
        ComponentCommand::Start { component_name } => {
            println!("  {} [Cluaiz {}] Starting daemon for: {}", "🚀".cyan(), component_type.to_uppercase(), component_name.bold());
        }
        ComponentCommand::Link { plugin_name, skill_name } => {
            println!("  {} [Cluaiz Plugin] Linking {} to {}", "🔗".cyan(), plugin_name.bold(), skill_name.bold());
        }
    }
    Ok(())
}

async fn handle_cache_command(component_type: &str, command: crate::ComponentCacheCommand) -> Result<()> {
    match command {
        crate::ComponentCacheCommand::Ls => {
            println!("\n  {} [Cluaiz Dual-Cache] Scanning Global {} Memory...", "🧠".cyan(), component_type.to_uppercase());
            match ToolHubInstaller::list_component_cache(component_type) {
                Ok(report) => println!("{}", report),
                Err(e) => println!("Error listing cache: {}", e),
            }
        }
        crate::ComponentCacheCommand::Clear { component_id, all, force } => {
            println!("\n  {} [Cluaiz Dual-Cache] Initiating Global Wipe for {}...", "🧹".yellow(), component_type.to_uppercase());
            match ToolHubInstaller::clear_component_cache(component_type, component_id, all, force) {
                Ok(wiped) => println!("\n    Successfully wiped {} caches.\n", wiped),
                Err(e) => println!("Error clearing cache: {}", e),
            }
        }
    }
    Ok(())
}

async fn install_component(component_type: &str, component_name: &str) -> Result<()> {
    // 🎨 Render Sovereign ASCII Logo Banner
    let logo = crate::assets::logos::logo_gallery::LOGO_VARIANTS[9];
    println!("\n{}", logo.cyan());

    println!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓");
    println!("┃ {} {}                        ", "📦 CLUAIZ HUB INSTALLER —".bold().cyan(), component_type.to_uppercase().bold().yellow());
    println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫");
    println!("┃ 🏷️  Target Package:  {}", component_name.bold().green());
    println!("┃ 🧩 Category:        {}", component_type.to_uppercase().cyan());
    println!("┃ 🌐 Hub Registry:    https://raw.githubusercontent.com/cluaiz/cluaiz-hub");
    println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛");
    println!();

    println!("  {} Resolving package metadata from Cluaiz Hub...", "🔍".cyan());
    
    match ToolHubInstaller::install_component(component_type, component_name).await {
        Ok(_) => {
            let tool_id = component_name.split('@').next().unwrap_or(component_name);
            let tool_opt = ToolsEngine::get_tool(tool_id).ok().flatten();

            println!("\n┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓");
            println!("┃ {}                                 ", "✅ COMPONENT SUCCESSFULLY INSTALLED & ACTIVE".bold().green());
            println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫");
            println!("┃ 🆔 Tool ID:       {}", tool_id.bold().cyan());
            
            if let Some(tool) = tool_opt {
                println!("┃ 📌 Version:       v{}", tool.version.yellow());
                println!("┃ ⚡ Mode:          {:?} (Context Router Auto-Trigger)", tool.execution_mode);
                let turns_str = if tool.default_turns == -1 {
                    "Permanent (-1)".to_string()
                } else if tool.default_turns == 0 {
                    "Ephemeral (0 turns)".to_string()
                } else {
                    format!("{} turns countdown", tool.default_turns)
                };
                println!("┃ ⏳ Lifespan:      {}", turns_str.cyan());
                if !tool.semantic_triggers.is_empty() {
                    println!("┃ 🎯 Triggers:      {:?}", tool.semantic_triggers);
                }
                println!("┃ 📂 Directory:     {}", tool.local_dir.bright_black());
            } else {
                println!("┃ 📌 Status:        Registered in tools_registry.json");
            }
            println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛");
            println!("  💡 To list all installed components, run: `cluaiz {} list`\n", component_type);
        }
        Err(e) => {
            println!("\n┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓");
            println!("┃ {}                                              ", "❌ INSTALLATION FAILED".bold().red());
            println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫");
            println!("┃ ⚠️  Reason: {}", e.to_string().yellow());
            println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\n");
        }
    }
    Ok(())
}

async fn remove_component(component_type: &str, component_name: &str) -> Result<()> {
    println!("  {} [Cluaiz {}] Removing: {}", "🗑️".cyan(), component_type.to_uppercase(), component_name.bold());
    if let Err(e) = ToolHubInstaller::remove_component(component_type, component_name).await {
        println!("  {} Error removing {}: {}", "❌".red(), component_type, e);
    } else {
        println!("  {} Successfully removed {} '{}' from tools_registry.json", "✅".green(), component_type, component_name.bold());
    }
    Ok(())
}

async fn list_components(component_type: &str) -> Result<()> {
    println!("\n  {} [Cluaiz Hub] Installed Sovereign {}:", "📦".cyan(), component_type.to_uppercase());
    if let Ok(reg) = ToolsEngine::registry() {
        let tools: Vec<_> = reg.installed_tools.values().filter(|t| t.category == component_type).collect();
        if tools.is_empty() {
            println!("    No {} installed yet. Use `cluaiz {} install <name>`.\n", component_type, component_type);
        } else {
            for tool in tools {
                let status = if tool.enabled { "[ENABLED]".green() } else { "[DISABLED]".red() };
                let mode = format!("{:?}", tool.execution_mode).to_uppercase();
                let turns_str = if tool.default_turns == -1 {
                    "Permanent (-1)".to_string()
                } else if tool.default_turns == 0 {
                    "Ephemeral (0)".to_string()
                } else {
                    format!("{} turns", tool.default_turns)
                };
                println!("    {} {} (v{}) {} | Mode: {} | Lifespan: {}", 
                    "🔹".blue(), 
                    tool.id.bold(), 
                    tool.version, 
                    status, 
                    mode.yellow(), 
                    turns_str.cyan()
                );
                if !tool.semantic_triggers.is_empty() {
                    println!("       {} Triggers: {:?}", "↳".bright_black(), tool.semantic_triggers);
                }
            }
            println!();
        }
    } else {
        println!("    No {} installed yet.\n", component_type);
    }
    Ok(())
}
