use color_eyre::Result;
use tokio::sync::mpsc;
use engines::{DownloadEvent};
use crate::core::state::AppState;
use inquire::{Select, ui::{Attributes, RenderConfig, Styled, Color}};
use colored::Colorize;
use std::io::Write;
pub mod table;
pub mod details;
use crate::ui::apps::registry::table::{RegistryTable, ColumnWidths};

pub struct RegistryApp;

impl RegistryApp {
    pub fn show(state: &mut AppState, tx: &mpsc::UnboundedSender<DownloadEvent>) -> Result<()> {
        if state.sorted_models.is_empty() {
            state.sorted_models = engines::CoreRoster::get_recommendations(&state.hardware.to_hardware_truth(), state.ram_gb);
        }

        let total_ram = state.ram_gb;
        
        // ── cluaiz SORTING ──
        state.sorted_models.sort_by(|a, b| {
            let (_, score_a, _) = RegistryTable::calculate_health(a, &state.hardware);
            let (_, score_b, _) = RegistryTable::calculate_health(b, &state.hardware);
            score_b.cmp(&score_a)
        });

        // ── 📏 COMPUTE DYNAMIC WIDTHS ──
        let widths = ColumnWidths::compute(&state.sorted_models);
        let table_header = RegistryTable::get_header_string(&widths);

        // ── 🧭 NAVIGATION HISTORY LOG ──
        let mut history_segment: Option<String> = None;
        let base_path = format!("{} ❯ {}", "🏠︎".dimmed(), "Model List".dimmed());

        loop {
            let current_crumb = if let Some(ref seg) = history_segment {
                format!("{} ❯ {}", base_path, seg)
            } else {
                base_path.clone()
            };

            // ── 🧼 ATOMIC RENDER BLOCK ──
            println!("{}", current_crumb);
            println!("{}", table_header);

            let mut choices: Vec<String> = state.sorted_models.iter().enumerate()
                .map(|(i, m)| RegistryTable::format_row(i, m, &widths, &state.hardware))
                .collect();
            
            choices.push("↩  Cancel".to_string());

        let config = RenderConfig::default()
            .with_prompt_prefix(Styled::new("🔍 ").with_fg(Color::LightCyan))
            .with_answered_prompt_prefix(Styled::new("🔍 ").with_fg(Color::LightCyan))
            .with_highlighted_option_prefix(
                Styled::new("➤")
                    .with_fg(Color::LightCyan)
                    .with_attr(Attributes::BOLD),
            );

            let ans = Select::new("Search:", choices)
                .with_page_size(15)
                .with_render_config(config)
                .prompt();

            match ans {
                Ok(ans) => {
                    if ans == "↩  Cancel" { 
                        // Surgical cleanup: 1 (Header) + 2 (Search) = 3 lines (Leave Crumb for History)
                        for _ in 0..3 { print!("\x1B[1A\x1B[2K\r"); }
                        println!("{} {}", "↩".dimmed(), "Back".dimmed());
                        let _ = std::io::stdout().flush();
                        return Ok(()); 
                    }

                    let idx_str = ans.split_whitespace().next().unwrap_or("0").trim();
                    let idx = idx_str.parse::<usize>().unwrap_or(1).saturating_sub(1);

                    if let Some(rec) = state.sorted_models.get_mut(idx) {
                        let name = rec.manifest.name.clone();
                        let (_, score, _) = RegistryTable::calculate_health(rec, &state.hardware);
                        
                        if score == 0 {
                            println!("\n  {} {}", "⚪".white(), "Action Blocked: Model incompatible with hardware.".bold());
                            std::thread::sleep(std::time::Duration::from_millis(1200));
                            // Cleanup: 1 (Blocked Msg) + 4 (UI) = 5 lines
                            for _ in 0..5 { print!("\x1B[1A\x1B[2K\r"); }
                            continue; 
                        }

                        // Wipe Breadcrumb + Header + Search (4 lines total) before entering details
                        for _ in 0..4 { print!("\x1B[1A\x1B[2K\r"); }
                        let _ = std::io::stdout().flush();

                        let action = details::show_details(idx, rec, total_ram, &base_path, tx)?;

                        // ── 🔄 STATE SYNC: Refresh model list after returning from details ──
                        state.sorted_models = engines::CoreRoster::get_recommendations(&state.hardware.to_hardware_truth(), state.ram_gb);
                        state.sorted_models.sort_by(|a, b| {
                            let (_, score_a, _) = RegistryTable::calculate_health(a, &state.hardware);
                            let (_, score_b, _) = RegistryTable::calculate_health(b, &state.hardware);
                            score_b.cmp(&score_a)
                        });

                        match action {
                            Some(act) if act == "DELETE" => {
                                history_segment = Some(format!("{} ❯ {} {}", name.dimmed(), "🗑️".dimmed(), "Deleted".dimmed()));
                                continue; 
                            },
                            Some(act) if act == "BACK" => {
                                history_segment = Some(name.dimmed().to_string());
                                continue; 
                            },
                            _ => {
                                history_segment = Some(name.dimmed().to_string());
                                continue; 
                            }
                        }
                    }
                }
                Err(_) => {
                    // Surgical cleanup on Escape: 3 lines (Leave Crumb for History)
                    for _ in 0..3 { print!("\x1B[1A\x1B[2K\r"); }
                    println!("{} {}", "↩".dimmed(), "Back".dimmed());
                    let _ = std::io::stdout().flush();
                    break;
                }
            }
        }
        
        Ok(())
    }
}
