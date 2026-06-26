use color_eyre::Result;
use colored::Colorize;
use std::path::PathBuf;
use crate::SkillCommand;
use reqwest;
use std::fs;
use std::io::Write;
use neural_core::interfaces::router_contract::EmbeddingDriver;

pub async fn execute(command: SkillCommand) -> Result<()> {
    match command {
        SkillCommand::Install { skill_name } => {
            install_skill(&skill_name).await?;
        }
        SkillCommand::List => {
            list_skills().await?;
        }
        SkillCommand::Cache { command } => {
            handle_cache_command(command).await?;
        }
        SkillCommand::Remove { skill_name } => {
            println!("  {} [Cluaize Skills] Removing skill: {}", "🧠".cyan(), skill_name.bold());
            // TODO: Delegate to SkillRegistry
        }
    }
    Ok(())
}

async fn handle_cache_command(command: crate::SkillCacheCommand) -> Result<()> {
    match command {
        crate::SkillCacheCommand::Ls => {
            println!("\n  {} [Cluaize Dual-Cache] Scanning Global Skill Memory...", "ðŸ§ ".cyan());
            match engines::neural_foundry::registry::SkillRegistry::list_skills_cache() {
                Ok(report) => println!("{}", report),
                Err(e) => println!("Error listing cache: {}", e),
            }
        }
        crate::SkillCacheCommand::Clear { model_id, all, force } => {
            println!("\n  {} [Cluaize Dual-Cache] Initiating Global Wipe...", "ðŸ§¹".yellow());
            match engines::neural_foundry::registry::SkillRegistry::clear_skills_cache(model_id, all, force) {
                Ok(wiped) => println!("\n    Successfully wiped {} caches.\n", wiped),
                Err(e) => println!("Error clearing cache: {}", e),
            }
        }
    }
    Ok(())
}

async fn install_skill(skill_name: &str) -> Result<()> {
    if let Err(e) = engines::neural_foundry::registry::SkillRegistry::install_skill(skill_name).await {
        println!("Error installing skill: {}", e);
    }
    Ok(())
}

async fn list_skills() -> Result<()> {
    println!("\n  {} [Cluaize] Installed Sovereign Skills:", "ðŸ“¦".cyan());
    match engines::neural_foundry::registry::SkillRegistry::list_installed_skills() {
        Ok(skills) => {
            if skills.is_empty() {
                println!("    No skills installed yet. Use `cluaize skill install <name>`.");
            } else {
                for name in skills {
                    println!("    {} {}", "ðŸ”¹".blue(), name.bold());
                }
            }
        }
        Err(_) => {
            println!("    No skills installed yet.");
        }
    }
    println!();
    Ok(())
}
