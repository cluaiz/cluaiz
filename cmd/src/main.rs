//! `cluaiz`: run, serve and manage local models from a plain terminal (stdin / stdout, no screen
//! takeover), with llama.cpp's flag names for loading and sampling.

use std::io::IsTerminal;

use clap::{Parser, Subcommand};
use color_eyre::Result;
use colored::Colorize;

mod assets;
mod chat;
mod cli;
mod core;
mod ui;

use crate::core::bootstrapper::Bootstrapper;

#[derive(Parser)]
#[command(name = "cluaiz", bin_name = "cluaiz", about = "Run and serve local AI models", version, arg_required_else_help = true)]
pub struct Cli {
    /// Engine logs on stderr: -v info (with the full plan), -vv debug. The log file always has them.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Chat with a model, or answer one prompt (`cluaiz run <model> "<prompt>"`)
    Run(cli::run::RunArgs),
    /// Show a model's details and the plan it would get on this machine
    Show(cli::show::ShowArgs),
    /// List installed models
    #[command(visible_alias = "ls")]
    List,
    /// Download and register a model into the local vault.
    #[command(alias = "install", alias = "i")]
    Pull {
        /// Model ID (e.g. gemma2:2b, unsloth/Qwen3.5-4B-GGUF)
        model_id: String,
    },
    /// Remove a model from the local vault.
    #[command(alias = "remove", alias = "delete")]
    Rm {
        /// Model ID to remove
        model_id: String,
    },
    /// Show the model the running server has loaded
    Ps,
    /// Start the API server
    Serve,
    /// Measure prompt and generation speed (llama-bench)
    Bench(cli::bench::BenchArgs),
    /// View and change engine settings (settings.json)
    Config {
        #[command(subcommand)]
        command: cli::config_cmd::ConfigCommand,
    },
    /// Show the hardware the planner sees
    Status,
    /// Re-scan the hardware (after a hardware or driver change)
    Calibrate,
    /// Choose the default chat / vector model
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// View and change engine permissions (permission.json)
    Permission {
        #[command(subcommand)]
        command: Option<PermissionCommand>,
    },
    /// Manage API access tokens
    Token {
        #[command(subcommand)]
        command: TokenCommand,
    },
    /// Manage skills
    Skill {
        #[command(subcommand)]
        command: ComponentCommand,
    },
    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        command: ComponentCommand,
    },
    /// Manage Model Context Protocol (MCP) integrations
    Mcp {
        #[command(subcommand)]
        command: ComponentCommand,
    },
    /// Ingest a document for semantic chunking and RAG
    Ingest {
        /// The file path to ingest
        file_path: String,
    },
    /// Manage temporary storage and media of the running server
    #[command(alias = "store")]
    Storage(cli::storage::StorageCli),
    /// 🛠️ Sync compiled development artifacts (engines, drivers) to ~/.cluaiz manually
    DevSync {
        /// Target to sync (all, core, driver)
        #[arg(default_value = "all")]
        target: String,

        /// Specific driver name if target is 'driver' (e.g., llama, onnx)
        driver_name: Option<String>,

        /// Profile to sync (debug or release). Defaults to release.
        #[arg(long, default_value = "release")]
        profile: String,
    },
}

#[derive(Subcommand)]
pub enum PermissionCommand {
    /// WASM firewall: auto, strict or off
    Firewall { status: String },
    /// Usage telemetry in chat responses: on or off
    Telemetry { status: String },
    /// KV cache generation: on or off
    Kvcache { status: String },
    /// Vectorize user messages: on or off
    VectorizeInput { status: String },
    /// Vectorize AI responses: on or off
    VectorizeResponse { status: String },
    /// Load the chat model on first use instead of at server start: on or off
    LazyLoad { status: String },
    /// API server port
    Port { number: u16 },
}

#[derive(Subcommand)]
pub enum TokenCommand {
    /// Show active API token from permission.json
    Show {
        /// Show all configured API tokens in full plaintext
        #[arg(short, long)]
        all: bool,
    },
    /// Create a new API access token
    Create,
    /// List all configured API tokens
    #[command(alias = "ls")]
    List,
    /// Remove an API access token
    #[command(alias = "rm")]
    Remove {
        /// The token or token suffix to remove
        token: String,
    },
}

#[derive(Subcommand)]
pub enum ModelCommand {
    /// Set active chat model
    SetChat { model_id: String },
    /// Set active vector model
    SetVector { model_id: String },
}

#[derive(Subcommand)]
pub enum ComponentCommand {
    /// Install a component from the cluaiz-tools registry
    #[command(alias = "i")]
    Install {
        /// Name of the component to install (e.g., 'cluaiz-search' or 'cluaiz-search@0.1.0')
        component_name: String,
    },
    /// List all locally installed components
    #[command(alias = "ls")]
    List,
    /// Manage Global Dual-Cache Artifacts
    Cache {
        #[command(subcommand)]
        command: ComponentCacheCommand,
    },
    /// Remove an installed component
    #[command(alias = "rm")]
    Remove {
        component_name: String,
    },
    /// Start a component's background daemon (Extensions and MCP only)
    Start {
        component_name: String,
    },
    /// Link a plugin to a skill (Plugin only)
    Link {
        plugin_name: String,
        skill_name: String,
    },
    /// View or change a component's settings (asks for missing values)
    Config {
        /// Component id
        component_id: Option<String>,
        /// Setting name
        key: Option<String>,
        /// New value
        value: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ComponentCacheCommand {
    /// List all active and orphaned dual-caches
    Ls,
    /// Clear caches
    Clear {
        /// The cache ID to target (optional)
        component_id: Option<String>,

        /// Clear all caches globally
        #[arg(long)]
        all: bool,

        /// Force deletion even if active
        #[arg(short = 'f', long)]
        force: bool,
    },
}

/// Log file in the local Cluaiz folder; appended (a CLI call never wipes a running server's log),
/// started over once it passes 20 MiB.
fn open_log_file() -> Option<std::fs::File> {
    let path = cluaiz_core::EnvironmentManager::current().local_dir.join("cluaiz_Core.log");
    if std::fs::metadata(&path).map(|m| m.len() > 20 * 1024 * 1024).unwrap_or(false) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    std::fs::OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Engine logs (tracing) go to the log file, and to stderr with -v / for `serve`. stdout stays the
/// command's own output.
fn init_logging(verbose: u8, serving: bool) {
    use tracing_subscriber::{filter::{LevelFilter, Targets}, prelude::*};
    let console = match (verbose, serving) {
        (0, false) => LevelFilter::OFF,
        (0, true) | (1, _) => LevelFilter::INFO,
        _ => LevelFilter::DEBUG,
    };
    let file_level = if verbose >= 2 { LevelFilter::DEBUG } else { LevelFilter::INFO };
    // ONNX Runtime reports every memory-arena step at INFO; below -vv only its warnings are kept
    let ort_level = if verbose >= 2 { LevelFilter::DEBUG } else { LevelFilter::WARN };
    let levels = |level: LevelFilter| Targets::new().with_default(level).with_target("ort", ort_level.min(level));
    let file_layer = open_log_file().map(|f| {
        tracing_subscriber::fmt::layer().with_writer(std::sync::Mutex::new(f)).with_ansi(false).with_filter(levels(file_level))
    });
    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(false)
        .with_filter(levels(console));
    let _ = tracing_subscriber::registry().with(file_layer).with(console_layer).try_init();
}

fn on_off(value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(color_eyre::eyre::eyre!("'{}': use on or off", value)),
    }
}

/// Prints the error the way every command reports one, and exits with status 1.
fn fail(context: &str, error: impl std::fmt::Display) -> ! {
    eprintln!("{} {}: {}", "error:".red().bold(), context, error);
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<()> {
    // 🚀 SOVEREIGN GHOST EXECUTION GUARD
    if let Ok(current_exe) = std::env::current_exe() {
        let global_bin_dir = cluaiz_core::EnvironmentManager::current().local_dir.join("bin");
        if !current_exe.starts_with(&global_bin_dir) {
            eprintln!("  {} [Ghost Execution Detected] You are running a local binary at {:?}", "⚠️".yellow().bold(), current_exe);
            eprintln!("  {} To use the System, run the global 'cluaiz' command.\n", "💡".cyan());
        }
    }

    // ══ FORCE UTF-8 FOR WINDOWS CONSOLE ══
    #[cfg(windows)]
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
        }
        SetConsoleOutputCP(65001);
    }
    std::env::set_var("GGML_LOG_LEVEL", "ERROR");
    color_eyre::install()?;

    // Colours only on a terminal (NO_COLOR is honoured by `colored` itself)
    if !std::io::stdout().is_terminal() {
        colored::control::set_override(false);
    }

    let cli = Cli::parse_from(cli::args::normalize_llama_flags(std::env::args()));
    init_logging(cli.verbose, matches!(cli.command, CliCommand::Serve));

    // 🚀 Cluaiz BOOTSTRAP (Local Dev-Sync & Registry Verification)
    let is_dev_sync = std::env::args().any(|arg| arg == "dev-sync");
    if let Err(e) = Bootstrapper::ignite(is_dev_sync).await {
        eprintln!("\n  {} [Cluaiz] Bootstrap Failed: {}\n", "❌".red(), e);
        std::process::exit(1);
    }

    let verbose = cli.verbose > 0;

    match cli.command {
        CliCommand::Run(args) => {
            if let Err(e) = cli::run::execute(args, verbose).await {
                fail("run", e);
            }
        }
        CliCommand::Show(args) => {
            if let Err(e) = cli::show::execute(args, verbose).await {
                fail("show", e);
            }
        }
        CliCommand::List => {
            if let Err(e) = cli::list::execute().await {
                fail("list", e);
            }
        }
        CliCommand::Pull { model_id } => {
            if let Err(e) = crate::cli::pull::execute(&model_id).await {
                eprintln!("\n  {} [Cluaiz] Pull Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        CliCommand::Rm { model_id } => {
            if let Err(e) = crate::cli::rm::execute(&model_id).await {
                eprintln!("\n  {} [Cluaiz] Removal Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        CliCommand::Ps => {
            if let Err(e) = cli::ps::execute().await {
                fail("ps", e);
            }
        }
        CliCommand::Serve => {
            let perms = cluaiz_security::PermissionSchema::load();
            eprintln!("Starting the Cluaiz API server on http://localhost:{} ...", cluaiz_api::api_port(&perms));
            if let Err(e) = cluaiz_api::run_daemon().await {
                fail("serve", e);
            }
        }
        CliCommand::Bench(args) => {
            if let Err(e) = cli::bench::execute(args, verbose).await {
                fail("bench", e);
            }
        }
        CliCommand::Config { command } => {
            if let Err(e) = cli::config_cmd::execute(command) {
                fail("config", e);
            }
        }
        CliCommand::Status => cli::status::execute(),
        CliCommand::Calibrate => {
            let _ = cluaiz_hw::HardwareProber::calibrate();
            if let Err(err) = onnx_engine::cudnn_driver::provision_cuda_runtime_at_setup() {
                tracing::warn!("CUDA/cuDNN 9 runtime provisioning during calibrate returned: {}", err);
            }
            println!("Hardware profile re-scanned.");
            cli::status::execute();
        }
        CliCommand::Model { command } => match command {
            ModelCommand::SetChat { model_id } => {
                cluaiz_security::PermissionSchema::set_active_chat_model(model_id.clone());
                println!("Active chat model: {}", model_id);
            }
            ModelCommand::SetVector { model_id } => {
                cluaiz_security::PermissionSchema::set_active_embedding_model(model_id.clone());
                println!("Active vector model: {}", model_id);
            }
        },
        CliCommand::Permission { command } => {
            let mut schema = cluaiz_security::PermissionSchema::load();
            let Some(cmd) = command else {
                let row = |k: &str, v: String| println!("{:<20}{}", k, v);
                row("firewall", schema.wasm_firewall.clone());
                row("telemetry", schema.stream_telemetry.to_string());
                row("kvcache", schema.enable_kvcache.to_string());
                row("vectorize-input", schema.vectorize_user_input.to_string());
                row("vectorize-response", schema.vectorize_ai_response.to_string());
                row("lazy-load", schema.lazy_load_model.to_string());
                row("port", schema.api_port.to_string());
                row("chat model", schema.get_active_chat_model().unwrap_or_else(|| "none".into()));
                row("vector model", schema.get_active_embedding_model().unwrap_or_else(|| "none".into()));
                eprintln!("{}", "Change one with `cluaiz permission <name> <value>`.".dimmed());
                return Ok(());
            };
            let result: Result<String> = (|| {
                Ok(match cmd {
                    PermissionCommand::Firewall { status } => {
                        schema.wasm_firewall = match status.to_lowercase().as_str() {
                            s @ ("auto" | "strict" | "off") => s.to_string(),
                            _ => return Err(color_eyre::eyre::eyre!("'{}': use auto, strict or off", status)),
                        };
                        format!("firewall = {}", schema.wasm_firewall)
                    }
                    PermissionCommand::Telemetry { status } => {
                        schema.stream_telemetry = on_off(&status)?;
                        format!("telemetry = {}", schema.stream_telemetry)
                    }
                    PermissionCommand::Kvcache { status } => {
                        schema.enable_kvcache = on_off(&status)?;
                        format!("kvcache = {}", schema.enable_kvcache)
                    }
                    PermissionCommand::VectorizeInput { status } => {
                        schema.vectorize_user_input = on_off(&status)?;
                        format!("vectorize-input = {}", schema.vectorize_user_input)
                    }
                    PermissionCommand::VectorizeResponse { status } => {
                        schema.vectorize_ai_response = on_off(&status)?;
                        format!("vectorize-response = {}", schema.vectorize_ai_response)
                    }
                    PermissionCommand::LazyLoad { status } => {
                        schema.lazy_load_model = on_off(&status)?;
                        format!("lazy-load = {}", schema.lazy_load_model)
                    }
                    PermissionCommand::Port { number } => {
                        schema.api_port = number;
                        format!("port = {}", schema.api_port)
                    }
                })
            })();
            match result.and_then(|msg| schema.save().map(|_| msg).map_err(|e| color_eyre::eyre::eyre!("{}", e))) {
                Ok(msg) => println!("{}", msg),
                Err(e) => fail("permission", e),
            }
        }
        CliCommand::Token { command } => {
            let mut schema = cluaiz_security::PermissionSchema::load();
            match command {
                TokenCommand::Show { all } => {
                    schema.api_auth.sanitize_tokens();
                    if schema.api_auth.tokens.is_empty() {
                        println!("\n  {} No API tokens configured in permission.json.", "⚠️".yellow());
                        println!("  {} Run '{}' to create an access token.\n", "💡".cyan(), "cluaiz token create".bold().green());
                        return Ok(());
                    }
                    if all {
                        println!("\n  {} All Configured API Tokens ({}/5):", "🔐".cyan().bold(), schema.api_auth.tokens.len());
                        for (idx, token) in schema.api_auth.tokens.iter().enumerate() {
                            let tag = if idx == 0 { " (primary active)".green() } else { "".normal() };
                            println!("    {}. {}{}", idx + 1, token.bold().green(), tag);
                        }
                        println!("  {} Auth Guard: {}\n", "🛡️".blue(), if schema.api_auth.required { "Enabled".green() } else { "Disabled (Local only)".yellow() });
                    } else {
                        let active_token = &schema.api_auth.tokens[0];
                        println!("\n  {} Active API Token: {}", "🔑".yellow(), active_token.bold().green());
                        println!("  {} Auth Guard: {}", "🛡️".blue(), if schema.api_auth.required { "Enabled".green() } else { "Disabled (Local only)".yellow() });
                        println!("  {} Total Configured Tokens: {} (run 'cluaiz token show --all' to see all)\n", "🔢".cyan(), schema.api_auth.tokens.len());
                    }
                }
                TokenCommand::Create => {
                    schema.api_auth.sanitize_tokens();
                    if schema.api_auth.tokens.len() >= 5 {
                        fail("token", "maximum of 5 API keys reached; remove one first");
                    }
                    let new_token = cluaiz_security::ApiAuth::generate_token();
                    schema.api_auth.tokens.push(new_token.clone());
                    schema.api_auth.required = true;
                    if let Err(e) = schema.save() {
                        fail("token", e);
                    }
                    println!("\n  {} New API Token Created Successfully!", "✨".green().bold());
                    println!("  {} Token: {}", "🔑".yellow(), new_token.bold().green());
                    println!("  {} Auth Guard: Enabled\n", "🛡️".blue());
                }
                TokenCommand::List => {
                    schema.api_auth.sanitize_tokens();
                    println!("\n  {} Configured API Tokens ({}/5):", "🔐".cyan().bold(), schema.api_auth.tokens.len());
                    if schema.api_auth.tokens.is_empty() {
                        println!("    No API tokens configured. Run 'cluaiz token create' to add one.");
                    } else {
                        for (idx, token) in schema.api_auth.tokens.iter().enumerate() {
                            let tag = if idx == 0 { " (primary)".green() } else { "".normal() };
                            println!("    {}. {}{}", idx + 1, token.bold().green(), tag);
                        }
                    }
                    println!();
                }
                TokenCommand::Remove { token } => match schema.api_auth.remove_token(&token) {
                    Ok(_) => {
                        if schema.api_auth.tokens.is_empty() {
                            schema.api_auth.required = false;
                        }
                        if let Err(e) = schema.save() {
                            fail("token", e);
                        }
                        println!("  {} Token removed successfully.", "✅".green());
                    }
                    Err(err) => fail("token", err),
                },
            }
        }
        CliCommand::Skill { command } => {
            if let Err(e) = cli::component::execute("skill", command).await {
                fail("skill", e);
            }
        }
        CliCommand::Plugin { command } => {
            if let Err(e) = cli::component::execute("plugin", command).await {
                fail("plugin", e);
            }
        }
        CliCommand::Mcp { command } => {
            if let Err(e) = cli::component::execute("mcp", command).await {
                fail("mcp", e);
            }
        }
        CliCommand::Ingest { file_path } => {
            if let Err(e) = cli::ingest::execute(&file_path).await {
                fail("ingest", e);
            }
        }
        CliCommand::Storage(cli_cmd) => cli::storage::handle_storage_command(cli_cmd).await,
        CliCommand::DevSync { target, driver_name, profile } => {
            let env_mgr = cluaiz_core::EnvironmentManager::current();
            let global_dir = env_mgr.global_dir.clone();
            let local_dir = env_mgr.local_dir.clone();
            println!("⚙️  [DevSync] Manually synchronizing '{}' development artifacts to {}...", target, global_dir.display());
            let _ = std::fs::create_dir_all(&global_dir);
            core::bootstrapper::Bootstrapper::sync_dev_artifacts(&target, driver_name.as_deref(), global_dir.clone(), &profile)?;
            if local_dir != global_dir {
                println!("⚙️  [DevSync] Also synchronizing to local workspace {}...", local_dir.display());
                let _ = core::bootstrapper::Bootstrapper::sync_dev_artifacts(&target, driver_name.as_deref(), local_dir, &profile);
            }

            std::env::set_var("cluaiz_HOME", global_dir.to_string_lossy().to_string());
            let permissions = cluaiz_security::PermissionSchema::load();
            let _ = permissions.save();
            let _ = cluaiz_hw::HardwareProber::current();

            std::env::remove_var("cluaiz_HOME");

            println!("✅  [DevSync] Synchronization Complete.");
        }
    }

    Ok(())
}
