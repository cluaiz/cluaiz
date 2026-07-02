use crate::core::state::AppState;
use crate::app_enums::Mode;
use color_eyre::Result;
use colored::Colorize;
use engines::DownloadEvent;
use inquire::{Select, ui::{RenderConfig, Styled, Color, Attributes}};
use std::io::{stdout, Write};
use tokio::sync::mpsc;

pub async fn run_native(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<DownloadEvent>,
    mode: &mut Mode,
) -> Result<()> {
    // ── 1. Unified Interface Initialization ──
    if !state.printed_logo {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        print!("\x1B[2J\x1B[1;1H"); // Clear and home
        crate::assets::logos::logo::print_native_logo(state.logo_index);
        println!();
        println!("  {} {}", "cluaiz".cyan().bold(), "v0.1.0".bright_black());
        if state.is_client_mode {
            println!("  {} {}", "Mode:        ".dimmed(), "Pure Client (Connected to Background API)".green().bold());
        } else {
            println!("  {} {}", "Mode:        ".dimmed(), "Standalone (Local Engine)".yellow().bold());
        }
        state.printed_logo = true;
    }

    loop {
        let config = RenderConfig::default()
            .with_prompt_prefix(Styled::new("🏠︎").with_fg(Color::LightCyan))
            .with_highlighted_option_prefix(
                Styled::new("⮞")
                    .with_fg(Color::LightCyan)
                    .with_attr(Attributes::BOLD),
            );

        let mut options = vec![
            "💬 Start New Chat",
            "🌐 Server Control",
            "🧠 Model Hub",
            "⚡ System & Hardware",
            "🛠️ Advanced Utilities",
            "ℹ️ Help",
            "❌ Quit",
        ];

        let ans = match Select::new("cluaiz Main Menu:", options)
            .with_render_config(config.clone())
            .prompt() {
            Ok(ans) => ans,
            Err(_) => {
                *mode = Mode::Quit;
                return Ok(());
            }
        };

        print!("\x1B[1A\x1B[2K\r");
        stdout().flush()?;

        match ans {
            "💬 Start New Chat" => {
                state.os_state = crate::core::state::OsState::Dashboard;
                return Ok(());
            }
            "🌐 Server Control" => {
                let mut s_opts = vec![];
                if !state.is_client_mode {
                    s_opts.push("🚀 Start API Daemon");
                }
                s_opts.push("👀 View Active Engines");
                s_opts.push("🔙 Back");
                
                if let Ok(s_ans) = Select::new("Server Control:", s_opts).with_render_config(config.clone()).prompt() {
                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                    match s_ans {
                        "🚀 Start API Daemon" => {
                            println!("  {} Starting cluaiz API Daemon on http://localhost:8000 ...", "🚀".green());
                            cluaiz_api::run_daemon().await;
                        }
                        "👀 View Active Engines" => {
                            let _ = crate::cli::ps::execute().await;
                        }
                        _ => {}
                    }
                }
            }
            "🧠 Model Hub" => {
                let m_opts = vec!["⬇️ Pull New Model", "🗑️ Delete Downloaded Model", "🔙 Back"];
                if let Ok(m_ans) = Select::new("Model Hub:", m_opts).with_render_config(config.clone()).prompt() {
                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                    match m_ans {
                        "⬇️ Pull New Model" => {
                            if let Ok(id) = inquire::Text::new("Enter Model ID to Pull:").prompt() {
                                let _ = crate::cli::pull::execute(&id).await;
                            }
                        }
                        "🗑️ Delete Downloaded Model" => {
                            let roster = engines::models::registry::CoreRoster::load_roster();
                            let mut downloaded: Vec<_> = roster.into_iter().filter(|m| {
                                m.local_path.is_some() || engines::models::fetch::ModelDownloader::get_cached_path(&m.category, &m.id, &m.huggingface_filename).is_some()
                            }).collect();
                            
                            downloaded.sort_by(|a, b| a.name.cmp(&b.name));
                            downloaded.dedup_by(|a, b| a.name == b.name);
                            
                            if downloaded.is_empty() {
                                println!("  {} No downloaded models found.", "ℹ️".blue());
                                std::thread::sleep(std::time::Duration::from_secs(2));
                            } else {
                                let options: Vec<String> = downloaded.iter().map(|m| format!("{} [{}]", m.name, m.architecture_type)).collect();
                                if let Ok(ans) = Select::new("Select Model to Delete:", options).with_render_config(config.clone()).prompt() {
                                    if let Some(model) = downloaded.iter().find(|m| format!("{} [{}]", m.name, m.architecture_type) == ans) {
                                        let _ = crate::cli::rm::execute(&model.id).await;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            "⚡ System & Hardware" => {
                let h_opts = vec!["⚙️ System Booster Config", "🛡️ Firewall & Permissions", "📊 Hardware Status & Health", "🔄 Re-calibrate Hardware", "⏱️ Run Benchmark", "🔙 Back"];
                if let Ok(h_ans) = Select::new("System & Hardware:", h_opts).with_render_config(config.clone()).prompt() {
                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                    match h_ans {
                        "⚙️ System Booster Config" => {
                            let _ = crate::cli::booster::execute(None, None, None, None).await;
                        }
                        "🛡️ Firewall & Permissions" => {
                            loop {
                                let mut perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                let opts = vec![
                                    format!("WASM Firewall (Current: {})", perms.wasm_firewall),
                                    format!("Telemetry (Current: {})", perms.stream_telemetry),
                                    format!("Lazy Load Model (Current: {})", perms.lazy_load_model),
                                    format!("Vectorize User Input (Current: {})", perms.vectorize_user_input),
                                    format!("Vectorize AI Response (Current: {})", perms.vectorize_ai_response),
                                    format!("Temporary Chat TTL (Current: {})", if perms.temporary_chat_ttl_hours == u64::MAX { "max".to_string() } else { format!("{} hours", perms.temporary_chat_ttl_hours) }),
                                    format!("Active Chat Model (Current: {})", perms.get_active_chat_model().unwrap_or_else(|| "None".to_string())),
                                    format!("Active Vector Model (Current: {})", perms.get_active_embedding_model().unwrap_or_else(|| "None".to_string())),
                                    "🔙 Back".to_string()
                                ];
                                if let Ok(ans) = Select::new("Permission Control:", opts).with_render_config(config.clone()).prompt() {
                                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                                    if ans == "🔙 Back" { break; }
                                    let key = ans.split(" (").next().unwrap_or("");
                                    let mut changed = false;
                                    match key {
                                        "WASM Firewall" => {
                                            let vals = vec!["auto".to_string(), "strict".to_string(), "off".to_string()];
                                            if let Ok(v) = Select::new("Set WASM Firewall:", vals).with_render_config(config.clone()).prompt() {
                                                perms.wasm_firewall = v;
                                                changed = true;
                                            }
                                        }
                                        "Telemetry" => {
                                            let vals = vec!["true".to_string(), "false".to_string()];
                                            if let Ok(v) = Select::new("Set Telemetry:", vals).with_render_config(config.clone()).prompt() {
                                                perms.stream_telemetry = v == "true";
                                                changed = true;
                                            }
                                        }
                                        "Lazy Load Model" => {
                                            let vals = vec!["true".to_string(), "false".to_string()];
                                            if let Ok(v) = Select::new("Set Lazy Load Model:", vals).with_render_config(config.clone()).prompt() {
                                                perms.lazy_load_model = v == "true";
                                                changed = true;
                                            }
                                        }
                                        "Vectorize User Input" => {
                                            let vals = vec!["true".to_string(), "false".to_string()];
                                            if let Ok(v) = Select::new("Set Vectorize User Input:", vals).with_render_config(config.clone()).prompt() {
                                                perms.vectorize_user_input = v == "true";
                                                changed = true;
                                            }
                                        }
                                        "Vectorize AI Response" => {
                                            let vals = vec!["true".to_string(), "false".to_string()];
                                            if let Ok(v) = Select::new("Set Vectorize AI Response:", vals).with_render_config(config.clone()).prompt() {
                                                perms.vectorize_ai_response = v == "true";
                                                changed = true;
                                            }
                                        }
                                        "Temporary Chat TTL" => {
                                            let vals = vec![
                                                "12 hr".to_string(),
                                                "24 hr".to_string(),
                                                "48 hr".to_string(),
                                                "72 hr".to_string(),
                                                "1 week".to_string(),
                                                "max".to_string(),
                                            ];
                                            if let Ok(v) = Select::new("Select TTL:", vals).with_render_config(config.clone()).prompt() {
                                                let hours = match v.as_str() {
                                                    "12 hr" => 12,
                                                    "24 hr" => 24,
                                                    "48 hr" => 48,
                                                    "72 hr" => 72,
                                                    "1 week" => 168,
                                                    "max" => u64::MAX,
                                                    _ => 24,
                                                };
                                                perms.temporary_chat_ttl_hours = hours;
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
                                                std::thread::sleep(std::time::Duration::from_secs(2));
                                            } else {
                                                let is_vector = key == "Active Vector Model";
                                                let filtered: Vec<_> = downloaded.into_iter().filter(|m| {
                                                    let is_model_vector = m.architecture_type == "onnx" || m.category == "embedding";
                                                    if is_vector { is_model_vector } else { !is_model_vector }
                                                }).collect();
                                                
                                                if filtered.is_empty() {
                                                    println!("  {} No downloaded {} found.", "ℹ️".blue(), if is_vector { "Vector Models" } else { "Chat Models" });
                                                    std::thread::sleep(std::time::Duration::from_secs(2));
                                                } else {
                                                    let options: Vec<String> = filtered.iter().map(|m| m.name.clone()).collect();
                                                    if let Ok(ans) = Select::new("Select Model to Set as Active:", options).with_render_config(config.clone()).prompt() {
                                                        if let Some(model) = filtered.iter().find(|m| m.name == ans) {
                                                            if is_vector {
                                                                engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_embedding_model(model.id.clone());
                                                            } else {
                                                                engines::neural_foundry::security::permission_schema::PermissionSchema::set_active_chat_model(model.id.clone());
                                                            }
                                                            perms = engines::neural_foundry::security::permission_schema::PermissionSchema::load();
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                                    if changed {
                                        perms.save();
                                        println!("  {} Permission.json updated.", "✅".green());
                                    }
                                } else {
                                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                                    break;
                                }
                            }
                        }
                        "📊 Hardware Status & Health" => {
                            engines::telemetry::health_check::cluaizHealthChecker::run_full_benchmark();
                        }
                        "🔄 Re-calibrate Hardware" => {
                            println!("\n  {} [Silicon] Initiating Hardware Re-Scan...", "🛠️".cyan());
                            engines::hardware::system_control_manager::detect_hardware();
                            println!("  {} [Silicon] SiliconTruth profile synchronized.\n", "✅".green());
                        }
                        "⏱️ Run Benchmark" => {
                            let _ = crate::cli::benchmark::execute(None, 1).await;
                        }
                        _ => {}
                    }
                }
            }
            "🛠️ Advanced Utilities" => {
                let u_opts = vec!["🧩 Manage Skills", "📄 Ingest Document", "⚙️ Setup Profile", "🔙 Back"];
                if let Ok(u_ans) = Select::new("Advanced Utilities:", u_opts).with_render_config(config.clone()).prompt() {
                    print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                    match u_ans {
                        "🧩 Manage Skills" => {
                            let s_opts = vec!["List Installed Skills", "Install Skill", "Manage Skill Caches", "🔙 Back"];
                            if let Ok(s_ans) = Select::new("Skill Manager:", s_opts).with_render_config(config.clone()).prompt() {
                                print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                                match s_ans {
                                    "List Installed Skills" => { let _ = crate::cli::component::execute("skill", crate::ComponentCommand::List).await; }
                                    "Install Skill" => {
                                        if let Ok(name) = inquire::Text::new("Enter Skill Name (e.g. web-search-github):").with_render_config(config.clone()).prompt() {
                                            let _ = crate::cli::component::execute("skill", crate::ComponentCommand::Install { component_name: name }).await;
                                        }
                                    }
                                    "Manage Skill Caches" => {
                                        let c_opts = vec!["List Caches", "Clear All Orphaned", "Clear Specific Cache"];
                                        if let Ok(c_ans) = Select::new("Cache Manager:", c_opts).with_render_config(config.clone()).prompt() {
                                            print!("\x1B[1A\x1B[2K\r"); stdout().flush()?;
                                            match c_ans {
                                                "List Caches" => { let _ = crate::cli::component::execute("skill", crate::ComponentCommand::Cache { command: crate::ComponentCacheCommand::Ls }).await; }
                                                "Clear All Orphaned" => { let _ = crate::cli::component::execute("skill", crate::ComponentCommand::Cache { command: crate::ComponentCacheCommand::Clear { component_id: None, all: true, force: false } }).await; }
                                                "Clear Specific Cache" => {
                                                    if let Ok(id) = inquire::Text::new("Enter Component ID to clear cache:").with_render_config(config.clone()).prompt() {
                                                        let _ = crate::cli::component::execute("skill", crate::ComponentCommand::Cache { command: crate::ComponentCacheCommand::Clear { component_id: Some(id), all: false, force: true } }).await;
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }

                        "📄 Ingest Document" => {
                            if let Ok(path) = inquire::Text::new("Enter File Path:").prompt() {
                                let _ = crate::cli::ingest::execute(&path).await;
                            }
                        }
                        "⚙️ Setup Profile" => {
                            let _ = crate::cli::setup::execute(crate::SetupCommand::Profile).await;
                        }
                        _ => {}
                    }
                }
            }
            "ℹ️ Help" => {
                if let Ok(reg) = crate::core::commands::CommandRegistry::load() {
                    reg.generate_help();
                } else {
                    println!("  \x1B[31mError loading commands.json\x1B[0m");
                }
            }
            "❌ Quit" => {
                *mode = Mode::Quit;
                return Ok(());
            }
            _ => {}
        }
    }
}
