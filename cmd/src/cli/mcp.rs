use color_eyre::Result;
use colored::Colorize;
use crate::McpCommand;
use engines::neural_foundry::registry::registry_index::MasterRegistry;

pub async fn execute(command: McpCommand) -> Result<()> {
    match command {
        McpCommand::Install { mcp_name } => {
            println!("  {} [Cluaize MCP] Installing MCP server: {}", "🔗".cyan(), mcp_name.bold());
            match engines::neural_foundry::registry::mcp_manager::McpManager::install_mcp(&mcp_name).await {
                Ok(_) => println!("  {} MCP server '{}' installed successfully.", "✅".green(), mcp_name.bold()),
                Err(e) => println!("  {} Failed to install MCP server: {}", "❌".red(), e),
            }
        }

        McpCommand::List => {
            println!("\n  {} [Cluaize MCP] Registered MCP Servers:", "🔗".cyan());
            match MasterRegistry::load() {
                Ok(registry) => {
                    if registry.mcp.is_empty() {
                        println!("    (No MCP servers registered)");
                    } else {
                        for (name, entry) in &registry.mcp {
                            let status = if entry.enabled { "✅".green() } else { "❌".red() };
                            let strategy = format!("{:?}", entry.load_strategy);
                            println!("    {} {} [{}] — {}", status, name.bold(), strategy.cyan(), entry.domain);
                        }
                    }
                }
                Err(_) => println!("    (No registry found — no MCP servers installed)"),
            }
        }

        McpCommand::Remove { mcp_name } => {
            println!("  {} [Cluaize MCP] Removing MCP server: {}", "🔗".cyan(), mcp_name.bold());
            match engines::neural_foundry::registry::mcp_manager::McpManager::remove_mcp(&mcp_name).await {
                Ok(_) => println!("  {} MCP server '{}' removed.", "✅".green(), mcp_name.bold()),
                Err(e) => println!("  {} Failed to remove MCP server: {}", "⚠️".yellow(), e),
            }
        }

        McpCommand::Cache { command } => {
            match command {
                crate::McpCacheCommand::Ls => {
                    println!("\n  {} [Cluaize MCP] MCP Cache Status:", "🔗".cyan());
                    match MasterRegistry::load() {
                        Ok(registry) => {
                            for (name, entry) in &registry.mcp {
                                let global_dir = cluaize_shared::environment::EnvironmentManager::current().global_dir;
                                let cache_path = global_dir.join(&entry.domain).join(".cache");
                                let cache_exists = if cache_path.exists() { "📦 cached".yellow() } else { "○ empty".dimmed() };
                                println!("    {} {} — {}", "🔗".cyan(), name.bold(), cache_exists);
                            }
                        }
                        Err(_) => println!("    (No registry found)"),
                    }
                }
                crate::McpCacheCommand::Clear { mcp_name, all } => {
                    println!("  {} [Cluaize MCP] Clearing MCP cache...", "🔗".cyan());
                    let target = if all { None } else { mcp_name.as_deref() };
                    match engines::neural_foundry::registry::mcp_manager::McpManager::clear_mcp_cache(target).await {
                        Ok(wiped) => println!("  {} Successfully wiped {} MCP cache(s).", "✅".green(), wiped),
                        Err(e) => println!("  {} Error clearing MCP cache: {}", "❌".red(), e),
                    }
                }
            }
        }

        McpCommand::Start { mcp_name } => {
            println!("  {} [Cluaize MCP] Starting MCP server for '{}'", "🚀".cyan(), mcp_name.bold());
            // TODO: Call McpManager::start_server when process spawning is implemented
        }
    }
    Ok(())
}
