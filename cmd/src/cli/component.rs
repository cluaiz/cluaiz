use color_eyre::Result;
use colored::Colorize;
use crate::ComponentCommand;
use cluaiz_agents::{ToolsInstaller, ToolsEngine};

pub async fn execute(component_type: &str, command: ComponentCommand) -> Result<()> {
    match command {
        ComponentCommand::Install { component_name, yes, no_recommended } => {
            install_component(component_type, &component_name, yes, no_recommended).await?;
        }
        ComponentCommand::Permission { component_id, permission, state } => {
            permission_command(component_type, &component_id, permission.as_deref(), state.as_deref())?;
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
        ComponentCommand::Config { component_id, key, value } => {
            crate::cli::component_config::execute(Some(component_type.to_string()), component_id, key, value)
                .await
                .map_err(|e| color_eyre::eyre::eyre!("{}", e))?;
        }
    }
    Ok(())
}

async fn handle_cache_command(component_type: &str, command: crate::ComponentCacheCommand) -> Result<()> {
    match command {
        crate::ComponentCacheCommand::Ls => {
            println!("\n  {} [Cluaiz Dual-Cache] Scanning Global {} Memory...", "🧠".cyan(), component_type.to_uppercase());
            match ToolsInstaller::list_component_cache(component_type) {
                Ok(report) => println!("{}", report),
                Err(e) => println!("Error listing cache: {}", e),
            }
        }
        crate::ComponentCacheCommand::Clear { component_id, all, force } => {
            println!("\n  {} [Cluaiz Dual-Cache] Initiating Global Wipe for {}...", "🧹".yellow(), component_type.to_uppercase());
            match ToolsInstaller::clear_component_cache(component_type, component_id, all, force) {
                Ok(report) => println!("\n    Removed {} cached files ({} bytes).\n", report.files, report.bytes),
                Err(e) => println!("Error clearing cache: {}", e),
            }
        }
    }
    Ok(())
}

async fn install_component(component_type: &str, component_name: &str, yes: bool, no_recommended: bool) -> Result<()> {
    println!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓");
    println!("┃ {} {}                        ", "📦 CLUAIZ TOOLS INSTALLER —".bold().cyan(), component_type.to_uppercase().bold().yellow());
    println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫");
    println!("┃ 🏷️  Target Package:  {}", component_name.bold().green());
    println!("┃ 🧩 Category:        {}", component_type.to_uppercase().cyan());
    println!("┃ 🌐 Tools Registry:  https://raw.githubusercontent.com/cluaiz/cluaiz-tools");
    println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛");
    println!();

    println!("  {} Resolving package metadata from Cluaiz Tools...", "🔍".cyan());
    
    let mut install_failed = false;
    match planned_install(component_type, component_name, yes, no_recommended).await {
        Ok(report) => {
            for line in &report {
                println!("  {} {}", "•".cyan(), line);
            }
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
            install_failed = true;
            println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\n");
        }
    }
    if install_failed {
        std::process::exit(1);
    }
    Ok(())
}

/// Plans the install, shows it, asks what the user's mode leaves open, then installs.
/// Without a terminal nothing is asked: questions count as no, unless `--yes`.
async fn planned_install(component_type: &str, component_name: &str, yes: bool, no_recommended: bool) -> color_eyre::Result<Vec<String>> {
    use cluaiz_agents::installer::plan::{Choice, Need, PermissionChoice};
    use std::io::IsTerminal;
    let plan = ToolsInstaller::plan_install(component_type, component_name)
        .await
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    println!("\n  {} Plan (mode: {}):", "🧭".cyan(), plan.mode.bold());
    for line in plan.describe() {
        println!("    - {line}");
    }
    let mut approved = cluaiz_agents::Approved::default();
    if plan.has_questions() {
        if yes {
            approved = cluaiz_agents::Approved::all(&plan);
        } else if std::io::stdin().is_terminal() {
            println!();
            for tool in plan.tools.iter().filter(|t| t.choice == Choice::Ask) {
                if no_recommended && tool.need == Need::Recommended {
                    continue;
                }
                let need = format!("{:?}", tool.need).to_lowercase();
                let question = format!("Install {} {} ({need} by {})? {}", tool.kind, tool.id, tool.needed_by, tool.info);
                let default = tool.need == Need::Required;
                if inquire::Confirm::new(&question).with_default(default).prompt().unwrap_or(false) {
                    approved.tools.insert(tool.key());
                }
            }
            for planned in plan.permissions.iter().filter(|p| p.choice == PermissionChoice::Ask) {
                let sensitive = if planned.permission.sensitive { " (sensitive)" } else { "" };
                let question = format!("Allow {}{} for {}? {}", planned.permission.title, sensitive, planned.tool, planned.permission.info);
                if inquire::Confirm::new(&question).with_default(!planned.permission.sensitive).prompt().unwrap_or(false) {
                    approved.permissions.insert(planned.key());
                }
            }
        } else {
            println!("  {} Not a terminal: questions count as no. Run with --yes to accept them.", "ℹ️".yellow());
        }
        if no_recommended {
            approved.tools.retain(|key| !plan.tools.iter().any(|t| t.key() == *key && t.need == Need::Recommended));
        }
    }
    ToolsInstaller::install_with_plan(&plan, &approved)
        .await
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))
}

/// `cluaiz <type> permission <id> [<permission> on|off]`
fn permission_command(component_type: &str, component_id: &str, permission: Option<&str>, state: Option<&str>) -> Result<()> {
    let Some(tool) = ToolsEngine::get_tool(component_id).ok().flatten().filter(|t| t.category == component_type) else {
        return Err(color_eyre::eyre::eyre!("{component_type} '{component_id}' is not installed."));
    };
    let manifest: serde_json::Value = std::fs::read_to_string(std::path::Path::new(&tool.local_dir).join("package.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    let config_dir = cluaiz_core::EnvironmentManager::current().config_dir();
    if let Some(kind) = permission {
        let on = match state {
            Some("on") => true,
            Some("off") => false,
            _ => return Err(color_eyre::eyre::eyre!("Say on or off: cluaiz {component_type} permission {component_id} {kind} on")),
        };
        if !cluaiz_agents::permissions::declared(&manifest).iter().any(|p| p.kind == kind) {
            return Err(color_eyre::eyre::eyre!("{component_type} '{component_id}' does not ask for '{kind}'."));
        }
        cluaiz_agents::permissions::set(&config_dir, component_type, component_id, kind, on).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        println!("  {} {kind} is now {} for {component_type} '{component_id}'", "✅".green(), if on { "on" } else { "off" });
        return Ok(());
    }
    let list = cluaiz_agents::permissions::effective(&config_dir, component_type, component_id, &manifest);
    if list.is_empty() {
        println!("  {component_type} '{component_id}' asks for no permissions.");
    }
    for p in list {
        let state = if p.on { "on".green() } else { "off".red() };
        let sensitive = if p.sensitive { ", sensitive" } else { "" };
        let chosen = if p.chosen { "your choice" } else { "default" };
        let scope = if p.options.is_empty() { String::new() } else { format!(" {}", serde_json::Value::Object(p.options.clone())) };
        println!("  {} {} ({}{sensitive}){scope}: {state} [{chosen}] - {}", "•".cyan(), p.title.bold(), p.kind, p.info);
    }
    println!("  Change one with: cluaiz {component_type} permission {component_id} <permission> on|off");
    Ok(())
}

async fn remove_component(component_type: &str, component_name: &str) -> Result<()> {
    println!("  {} [Cluaiz {}] Removing: {}", "🗑️".cyan(), component_type.to_uppercase(), component_name.bold());
    if let Err(e) = ToolsInstaller::remove_component(component_type, component_name).await {
        println!("  {} Error removing {}: {}", "❌".red(), component_type, e);
        std::process::exit(1);
    } else {
        println!("  {} Successfully removed {} '{}' from tools_registry.json", "✅".green(), component_type, component_name.bold());
    }
    Ok(())
}

async fn list_components(component_type: &str) -> Result<()> {
    println!("\n  {} [Cluaiz Tools] Installed {}:", "📦".cyan(), component_type.to_uppercase());
    let registry = ToolsEngine::registry();
    if let Some(kept) = cluaiz_agents::ToolsRegistry::recovered_from() {
        println!(
            "    {} tools_registry.json could not be read; it was kept as {} and the list was built again from the tool folders. Each tool's own settings (security mode, execution mode, trust, turns) are back to their defaults.",
            "⚠".yellow(),
            kept.display()
        );
    }
    if let Ok(reg) = registry {
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
