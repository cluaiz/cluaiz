#![allow(warnings)]

use color_eyre::Result;
use colored::Colorize;
use clap::{Parser, Subcommand, CommandFactory};

mod core;
mod ui;
mod assets;
mod theme;
mod app_enums;

mod cli;

use crate::core::bootstrapper::Bootstrapper;

// ── Cluaiz CLI Definition ──────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "cluaiz", about = "Cluaiz-OS: Sovereign Neural Kernel", version = env!("CARGO_PKG_VERSION"), disable_help_subcommand = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,

    /// Run a full hardware performance benchmark (legacy flag)
    #[arg(long = "benchmark")]
    legacy_benchmark: bool,

    /// Re-scan hardware and synchronize SiliconTruth profile (legacy flag)
    #[arg(long = "calibrate")]
    legacy_calibrate: bool,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Manage Sovereign AI Skills
    Skill {
        #[command(subcommand)]
        command: Option<crate::SkillCommand>,
    },
    /// Pull & run a model. Downloads if not cached.
    Run {
        /// Model ID (e.g. gemma2:2b, bonsai:8b)
        #[arg(default_value = "")]
        model_id: String,
        
        /// Enter interactive chat mode (Default: true)
        #[arg(short, long, default_value_t = true, action = clap::ArgAction::Set)]
        interactive: bool,
    },

    /// Open the Cluaize Main Menu.
    Menu,

    /// List all downloaded models in the vault.
    List,

    /// Download and register a model into the local vault.
    Pull {
        /// Model ID (e.g. gemma2:2b, unsloth/Qwen3.5-4B-GGUF)
        model_id: String,
    },

    /// Remove a model from the local vault.
    Rm {
        /// Model ID to remove
        model_id: String,
    },

    /// Show hardware status and silicon health.
    Status,

    /// Re-scan hardware and synchronize SiliconTruth profile.
    Calibrate,

    /// Run a full hardware performance benchmark.
    Benchmark {
        /// Optional model ID to benchmark (runs all if omitted)
        model_id: Option<String>,

        /// Number of times to run each prompt.
        #[arg(short, long, default_value_t = 1)]
        runs: usize,
    },

    /// Show the dynamic help screen.
    Help,

    /// Show active neural engines in memory.
    Ps,

    /// Start the background API Daemon (Server mode).
    Serve,

    /// View or configure the system performance booster settings.
    Booster {
        /// Set KV-Cache Quantization level (auto, kv16, kv8, kv4)
        #[arg(long)]
        kv_quant: Option<String>,

        /// Set Context Shifting / Sliding Window mode (auto, off, minimal, standard, aggressive, extreme)
        #[arg(long)]
        context_shift: Option<String>,

        /// Set execution performance mode (edge, multitasking, balance, max_boost, ultra_max_boost, hyper_cluster)
        #[arg(long)]
        mode: Option<String>,

        /// Enable/Disable Hybrid Speculative Decoding (on, off, auto)
        #[arg(long)]
        spec_decode: Option<String>,
    },

    /// Ingest a document natively for semantic chunking and RAG.
    Ingest {
        /// The file path to ingest
        file_path: String,
    },

    /// Test JIT KV Cache compilation and memory footprint
    TestJit,

    /// Manage the Cluaizd Brain Connection
    Brain {
        #[command(subcommand)]
        command: BrainCommand,
    },

    /// Manage Engine Permissions
    Permission {
        #[command(subcommand)]
        command: Option<PermissionCommand>,
    },

    /// Manage Models
    Model {
        #[command(subcommand)]
        command: Option<ModelCommand>,
    },

    /// Setup Cluaiz Node Profile and Identity
    Setup {
        #[command(subcommand)]
        command: SetupCommand,
    },
}

#[derive(Subcommand)]
pub enum PermissionCommand {
    /// Change the WASM firewall status (auto, strict, off)
    Firewall {
        status: String,
    },
    /// Change the Telemetry status (on, off)
    Telemetry {
        status: String,
    },
}

#[derive(Subcommand)]
pub enum ModelCommand {
    /// Set active chat model
    SetChat {
        model_id: String,
    },
    /// Set active vector model
    SetVector {
        model_id: String,
    },
}

#[derive(Subcommand)]
pub enum SetupCommand {
    /// Generate and register Purpose Vectorization for the Node Profile
    Profile,
}

#[derive(Subcommand)]
pub enum BrainCommand {
    /// Enable the FFI Database connection (defaults to local, or specify a remote address)
    On {
        /// Remote database IP:Port (e.g. 10.0.0.5:8080)
        address: Option<String>,
    },
    /// Disable the FFI Database connection
    Off,
    /// Pure Brain Mode: Enable local DB but suspend Engine LLM loading to save VRAM
    Only,
    /// View the connection status and background daemon health
    Status,
}

#[derive(Subcommand)]
enum SkillCommand {
    /// Install a skill from the cluaiz-skills registry
    Install {
        /// Name of the skill to install (e.g., 'web-search-github')
        skill_name: String,
    },
    /// List all locally installed skills
    List,
    /// Manage Global Dual-Cache Artifacts
    Cache {
        #[command(subcommand)]
        command: SkillCacheCommand,
    },
}

#[derive(Subcommand)]
pub enum SkillCacheCommand {
    /// List all active and orphaned dual-caches
    Ls,
    /// Clear orphaned caches (or target a specific model cache)
    Clear {
        /// The model cache ID to target (optional)
        model_id: Option<String>,
        
        /// Clear all orphaned caches globally
        #[arg(long)]
        all: bool,
        
        /// Force deletion even if model is active
        #[arg(short = 'f', long)]
        force: bool,
    },
}

// ──────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // 🚀 SOVEREIGN GHOST EXECUTION GUARD
    if let Ok(current_exe) = std::env::current_exe() {
        let global_bin_dir = cluaize_shared::HardwareGovernor::resolve_bin_gateway();
        if !current_exe.starts_with(&global_bin_dir) {
            eprintln!("  {} [Ghost Execution Detected] You are running a local binary at {:?}", "⚠️".yellow().bold(), current_exe);
            eprintln!("  {} To use the Sovereign System, run the global 'cluaiz' command.\n", "💡".cyan());
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
    // ══ SOVEREIGN KERNEL SILENCE ══
    // These env vars suppress CUDA Graph + ggml verbose logs at the source.
    // NOTE: We do NOT redirect stderr here because inquire (input box) uses stderr to render.
    // Stderr is selectively redirected only during inference (in dashboard.rs generate_stream).
    // 🚀 GGML_CUDA_USE_GRAPHS=1: Enables 40% speed boost.
    std::env::set_var("GGML_CUDA_USE_GRAPHS", "1");
    std::env::set_var("GGML_LOG_LEVEL", "ERROR");

    color_eyre::install()?;

    let cli = Cli::parse();

    // 🚀 SILENCE THE VOID: Redirect all logs to file at the project root
    let log_path = {
        let mut path = std::env::current_dir().unwrap_or_default();
        let mut root = None;
        for _ in 0..5 {
            if path.join("Apps").exists() && path.join("interface-engines").exists() {
                root = Some(path.clone());
                break;
            }
            if let Some(parent) = path.parent() {
                path = parent.to_path_buf();
            } else {
                break;
            }
        }
        if let Some(r) = root {
            r.join("cluaiz_Core.log")
        } else {
            std::path::PathBuf::from("cluaiz_Core.log")
        }
    };

    if let Ok(log_file) = std::fs::File::create(&log_path) {
        let _ = tracing_subscriber::fmt()
            .with_writer(log_file)
            .with_ansi(false)
            .try_init();
    }

    // 🚀 Cluaiz BOOTSTRAP (Local Dev-Sync & Registry Verification)
    if let Err(e) = Bootstrapper::ignite().await {
        eprintln!("\n  {} [Cluaiz] Bootstrap Failed: {}\n", "❌".red(), e);
        std::process::exit(1);
    }

    // 🚀 Check Pure Brain Mode
    let mut pure_brain = false;
    if let Ok(control) = cluaize_shared::hardware::governor::HardwareGovernor::load_system_control() {
        if control.brain.is_pure_brain() {
            pure_brain = true;
        }
    }

    // 🚀 SILICON IGNITION: Optimize hardware before execution
    if !pure_brain {
        let _ = engines::system_booster::SystemBooster::ignite();
    }

    // -- Legacy Flag Handlers --
    if cli.legacy_benchmark {
        if let Err(e) = crate::cli::benchmark::execute(None, 1).await {
            eprintln!("\n  {} [Cluaiz] Benchmark Error: {}\n", "❌".red(), e);
            std::process::exit(1);
        }
        return Ok(());
    }

    if cli.legacy_calibrate {
         println!("\n  {} [Silicon] Initiating Hardware Re-Scan...", "🛰️".cyan());
         engines::hardware::system_control_manager::detect_hardware();
         println!("  {} [Silicon] SiliconTruth profile synchronized.\n", "✅".green());
        return Ok(());
    }

    match cli.command {
        Some(CliCommand::Menu) => {
            start_menu().await?;
        }
        Some(CliCommand::Run { model_id, interactive }) => {
            if model_id.trim().is_empty() {
                start_dashboard().await?;
            } else {
                if let Err(e) = crate::cli::run::execute(&model_id, interactive).await {
                    eprintln!("\n  {} [Cluaiz] Run Error: {}\n", "❌".red(), e);
                    std::process::exit(1);
                }
            }
        }
        Some(CliCommand::Pull { model_id }) => {
            if let Err(e) = crate::cli::pull::execute(&model_id).await {
                eprintln!("\n  {} [Cluaiz] Pull Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::List) => {
            if let Err(e) = crate::cli::list::execute().await {
                eprintln!("\n  {} [Cluaiz] List Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Rm { model_id }) => {
            if let Err(e) = crate::cli::rm::execute(&model_id).await {
                eprintln!("\n  {} [Cluaiz] Removal Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Status) => {
            engines::telemetry::health_check::CluaizeHealthChecker::run_full_benchmark();
        }
        Some(CliCommand::Calibrate) => {
             println!("\n  {} [Silicon] Initiating Hardware Re-Scan...", "🛰️".cyan());
             engines::hardware::system_control_manager::detect_hardware();
             println!("  {} [Silicon] SiliconTruth profile synchronized.\n", "✅".green());
        }
        Some(CliCommand::Benchmark { model_id, runs }) => {
            if let Err(e) = crate::cli::benchmark::execute(model_id, runs).await {
                eprintln!("\n  {} [Cluaiz] Benchmark Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Help) => {
            if let Ok(reg) = crate::core::commands::CommandRegistry::load() {
                reg.generate_help();
            } else {
                println!("  {} Error loading commands.json", "❌".red());
            }
        }
        Some(CliCommand::Ps) => {
            if let Err(e) = crate::cli::ps::execute().await {
                eprintln!("\n  {} [Cluaiz] Process Status Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Serve) => {
            println!("  {} Starting Cluaize API Daemon on http://localhost:8000 ...", "🚀".green());
            cluaize_api::run_daemon().await; 
        }
        Some(CliCommand::Booster { kv_quant, context_shift, mode, spec_decode }) => {
            if let Err(e) = crate::cli::booster::execute(kv_quant, context_shift, mode, spec_decode).await {
                eprintln!("\n  {} [Cluaiz] Booster Config Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Skill { command }) => {
            if let Some(cmd) = command {
                if let Err(e) = crate::cli::skill::execute(cmd).await {
                    eprintln!("\n  {} [Cluaiz] Skill Manager Error: {}\n", "❌".red(), e);
                    std::process::exit(1);
                }
            } else {
                let mut cmd = crate::Cli::command();
                let _ = cmd.print_help();
                std::process::exit(2);
            }
        }
        Some(CliCommand::Ingest { file_path }) => {
            if let Err(e) = crate::cli::ingest::execute(&file_path).await {
                eprintln!("\n  {} [Cluaiz] Ingestion Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::TestJit) => {
            if let Err(e) = crate::cli::test_jit::execute().await {
                eprintln!("\n  {} [Cluaiz] JIT Test Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Brain { command }) => {
            if let Err(e) = crate::cli::brain::execute(command).await {
                eprintln!("\n  {} [Cluaiz] Brain Manager Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Setup { command }) => {
            if let Err(e) = crate::cli::setup::execute(command).await {
                eprintln!("\n  {} [Cluaiz] Setup Error: {}\n", "❌".red(), e);
                std::process::exit(1);
            }
        }
        Some(CliCommand::Permission { command }) => {
            let mut schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
            if let Some(cmd) = command {
                match cmd {
                    PermissionCommand::Firewall { status } => {
                        schema.wasm_firewall = match status.to_lowercase().as_str() {
                            "strict" => "strict".to_string(),
                            "off" => "off".to_string(),
                            _ => "auto".to_string(),
                        };
                        println!("  {} Firewall updated to: {}", "✅".green(), schema.wasm_firewall.bold());
                    }
                    PermissionCommand::Telemetry { status } => {
                        schema.stream_telemetry = status.to_lowercase() == "on";
                        println!("  {} Telemetry updated to: {}", "✅".green(), schema.stream_telemetry);
                    }
                }
                schema.save();
            } else {
                use inquire::Select;
                loop {
                    schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                    let opts = vec![
                        format!("WASM Firewall (Current: {})", schema.wasm_firewall),
                        format!("Telemetry (Current: {})", schema.stream_telemetry),
                        format!("Vectorize User Input (Current: {})", schema.vectorize_user_input),
                        format!("Vectorize AI Response (Current: {})", schema.vectorize_ai_response),
                        format!("Temporary Chat TTL (Current: {})", if schema.temporary_chat_ttl_hours == u64::MAX { "max".to_string() } else { format!("{} hours", schema.temporary_chat_ttl_hours) }),
                        format!("Active Chat Model (Current: {})", schema.get_active_chat_model().unwrap_or_else(|| "None".to_string())),
                        format!("Active Vector Model (Current: {})", schema.get_active_embedding_model().unwrap_or_else(|| "None".to_string())),
                        "Quit".to_string()
                    ];
                    if let Ok(ans) = Select::new("Permission Control:", opts).prompt() {
                        if ans == "Quit" { break; }
                        let key = ans.split(" (").next().unwrap_or("");
                        let mut changed = false;
                        match key {
                            "WASM Firewall" => {
                                if let Ok(v) = Select::new("Set WASM Firewall:", vec!["auto", "strict", "off"]).prompt() {
                                    schema.wasm_firewall = v.to_string();
                                    changed = true;
                                }
                            }
                            "Telemetry" => {
                                if let Ok(v) = Select::new("Set Telemetry:", vec!["true", "false"]).prompt() {
                                    schema.stream_telemetry = v == "true";
                                    changed = true;
                                }
                            }
                            "Vectorize User Input" => {
                                if let Ok(v) = Select::new("Set Vectorize User Input:", vec!["true", "false"]).prompt() {
                                    schema.vectorize_user_input = v == "true";
                                    changed = true;
                                }
                            }
                            "Vectorize AI Response" => {
                                if let Ok(v) = Select::new("Set Vectorize AI Response:", vec!["true", "false"]).prompt() {
                                    schema.vectorize_ai_response = v == "true";
                                    changed = true;
                                }
                            }
                            "Temporary Chat TTL" => {
                                if let Ok(v) = Select::new("Select TTL:", vec!["12 hr", "24 hr", "48 hr", "72 hr", "1 week", "max"]).prompt() {
                                    schema.temporary_chat_ttl_hours = match v {
                                        "12 hr" => 12, "24 hr" => 24, "48 hr" => 48, "72 hr" => 72, "1 week" => 168, "max" => u64::MAX, _ => 24,
                                    };
                                    changed = true;
                                }
                            }
                            "Active Chat Model" | "Active Vector Model" => {
                                let roster = engines::models::registry::CoreRoster::load_roster();
                                let mut downloaded: Vec<_> = roster.into_iter().filter(|m| {
                                    m.local_path.is_some() || engines::models::fetch::ModelDownloader::get_cached_path(&m.category, &m.id, &m.huggingface_filename).is_some()
                                }).collect();
                                
                                downloaded.sort_by(|a, b| a.name.cmp(&b.name));
                                downloaded.dedup_by(|a, b| a.name == b.name);
                                
                                if downloaded.is_empty() {
                                    println!("  {} No downloaded models found.", "ℹ️".blue());
                                } else {
                                    let is_vector = key == "Active Vector Model";
                                    let filtered: Vec<_> = downloaded.into_iter().filter(|m| {
                                        let is_model_vector = m.architecture_type == "onnx" || m.category == "embedding";
                                        if is_vector { is_model_vector } else { !is_model_vector }
                                    }).collect();
                                    
                                    if filtered.is_empty() {
                                        println!("  {} No downloaded {} found.", "ℹ️".blue(), if is_vector { "Vector Models" } else { "Chat Models" });
                                    } else {
                                        let options: Vec<String> = filtered.iter().map(|m| m.name.clone()).collect();
                                        if let Ok(ans) = Select::new("Select Model to Set as Active:", options).prompt() {
                                            if let Some(model) = filtered.iter().find(|m| m.name == ans) {
                                                if is_vector {
                                                    engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(model.id.clone());
                                                } else {
                                                    engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(model.id.clone());
                                                }
                                                // We must reload schema because the static function above modified and saved it to disk.
                                                schema = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                        if changed { schema.save(); println!("  {} Permission.json updated.", "✅".green()); }
                    } else {
                        break;
                    }
                }
            }
        }
        Some(CliCommand::Model { command }) => {
            if let Some(cmd) = command {
                match cmd {
                    ModelCommand::SetChat { model_id } => {
                        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(model_id.clone());
                        println!("  {} Active Chat Model set to: {}", "✅".green(), model_id.bold());
                    }
                    ModelCommand::SetVector { model_id } => {
                        engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(model_id.clone());
                        println!("  {} Active Vector Model set to: {}", "✅".green(), model_id.bold());
                    }
                }
            } else {
                let mut cmd = crate::Cli::command();
                let _ = cmd.print_help();
                std::process::exit(2);
            }
        }
        None => {
            // Default to Menu if no command provided
            start_menu().await?;
        }
    }

    Ok(())
}

async fn start_dashboard() -> Result<()> {
    // 📡 Hardware IGNITION
    engines::telemetry::ignite_watchtower();

    // 🛡️ Panic Guard: Ensure terminal recovery on crash
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crate::core::flow::FlowEngine::restore();
        let _ = ratatui::restore();
        hook(info);
    }));

    // ── Cluaiz PRIMARY FLOW ──
    let app = crate::core::app::App::new(None, None, Some(crate::core::state::OsState::Dashboard))?;
    app.run().await?;

    let _ = crate::core::flow::FlowEngine::restore();
    Ok(())
}

async fn start_menu() -> Result<()> {
    // 📡 Hardware IGNITION
    engines::telemetry::ignite_watchtower();

    // 🛡️ Panic Guard: Ensure terminal recovery on crash
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crate::core::flow::FlowEngine::restore();
        let _ = ratatui::restore();
        hook(info);
    }));

    // ── Cluaiz PRIMARY FLOW ──
    let app = crate::core::app::App::new(None, None, Some(crate::core::state::OsState::MainMenu))?;
    app.run().await?;

    let _ = crate::core::flow::FlowEngine::restore();
    Ok(())
}

