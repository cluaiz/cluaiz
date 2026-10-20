use color_eyre::Result;
use colored::Colorize;
use cluaiz_models::ModelRecommendation;
use inquire::{
    ui::{Attributes, Color as InqColor, RenderConfig, Styled},
    Select, Confirm,
};
use tokio::sync::mpsc;
use cluaiz_models::DownloadEvent;
use crossterm::event::{self, Event, KeyCode};
use std::time::Duration;
use std::io::{stdout, Write};

pub fn show_details(
    _idx: usize,
    rec: &mut ModelRecommendation,
    _total_ram: f64,
    breadcrumb: &str,
    tx: &mpsc::UnboundedSender<DownloadEvent>,
) -> Result<Option<String>> {
    let m = &rec.manifest;
    let mut lines_printed = 0;

    let size_str = if m.is_cloud_api {
        "N/A".to_string()
    } else {
        format!("{:.2} GB", m.download_size_gb)
    };

    let status_str = rec.status.to_uppercase();
    let status_col = match rec.status.as_str() {
        "Optimal" => colored::Color::Green,
        "Average" => colored::Color::Yellow,
        "Heavy" => colored::Color::Red,
        "Cloud" => colored::Color::BrightBlue,
        _ => colored::Color::BrightBlack,
    };

    let print_row = |label: &str, val: colored::ColoredString, lines: &mut i32| {
        println!("{}   {:<13} {}", "│".cyan(), label.bright_black(), val);
        *lines += 1;
    };

    // ── 🧭 NAVIGATION HISTORY LOG ──
    println!("{} ❯ {}", breadcrumb, m.name.dimmed());
    lines_printed += 1;

    // print_row("ID:", m.id.white(), &mut lines_printed); // Removed per USER request
    print_row("ARCH:", m.architecture.white(), &mut lines_printed);
    print_row("PARAMS:", m.parameters.cyan(), &mut lines_printed);
    print_row("TRAINING:", m.training_tokens.white(), &mut lines_printed);
    print_row("CONTEXT:", m.context_window.white(), &mut lines_printed);
    print_row("DISK SIZE:", size_str.white(), &mut lines_printed);
    print_row(
        "STATUS:",
        status_str.color(status_col).bold(),
        &mut lines_printed,
    );

    println!("{}", "│".cyan());
    lines_printed += 1;

    let desc_title = " Core DESCRIPTION ";
    let mid_line = format!("├─{}────────────────────────────────────", desc_title);
    println!("{}", mid_line.cyan());
    lines_printed += 1;

    let desc_words: Vec<&str> = m.description.split_whitespace().collect();
    let mut current_line = String::new();

    for word in desc_words {
        if current_line.len() + word.len() > 64 {
            println!("{} {}", "│".cyan(), current_line.white());
            lines_printed += 1;
            current_line.clear();
        }
        if !current_line.is_empty() {
            current_line.push(' ');
        }
        current_line.push_str(word);
    }
    if !current_line.is_empty() {
        println!("{} {}", "│".cyan(), current_line.white());
        lines_printed += 1;
    }

    println!(
        "{}",
        "└───────────────────────────────────────────────────────────────".cyan()
    );
    println!();
    lines_printed += 2;

    let config = RenderConfig::default()
        .with_prompt_prefix(Styled::new(""))
        // ── 🛡️ NO GHOST RECORDS: Disable answered styling ──
        .with_answered_prompt_prefix(Styled::new(""))
        .with_selected_option(None)
        .with_highlighted_option_prefix(
            Styled::new("➤")
                .with_fg(InqColor::LightGreen)
                .with_attr(Attributes::BOLD),
        );

    loop {
        let mut options = Vec::new();
        let mut back_btn_idx = 0;

        if !rec.is_cached {
            options.push("📥  INITIATE DOWNLOAD".to_string());
            back_btn_idx = 1;
        }

        options.push("↩  BACK".to_string());

        if rec.is_cached {
            options.push(format!("{}", "🗑️  DELETE MODEL".red().bold()));
        }

        let ans = Select::new("", options.clone())
            .with_render_config(config)
            .with_starting_cursor(back_btn_idx)
            .with_formatter(&|_| String::new())
            .prompt();

        match ans {
            Ok(choice) => {
                if choice.contains("INITIATE") {
                    // ── 📥 INTERNAL DOWNLOAD BLOCK ──
                    let m = &rec.manifest;
                    let mid = m.id.clone();
                    let tx_clone = tx.clone();

                    let abort = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let abort_clone = abort.clone();

                    let (local_tx, mut local_rx) = tokio::sync::mpsc::channel(256);

                    let install_manifest = m.clone();
                    tokio::spawn(async move {
                        // Same download + registration as `cluaiz pull`
                        let result = cluaiz_models::ModelDownloader::install(
                            &install_manifest, &[install_manifest.huggingface_filename.clone()], local_tx, abort_clone
                        ).await;
                        let event = match result {
                            Ok(_) => DownloadEvent::Complete(mid),
                            Err(error) => DownloadEvent::Error(mid, error),
                        };
                        let _ = tx_clone.send(event);
                    });

                    let start = std::time::Instant::now();
                    println!();

                    let mut aborted = false;
                    loop {
                        // 1. Check for cancellation input
                        if event::poll(Duration::from_millis(50))? {
                            if let Event::Key(key) = event::read()? {
                                if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') {
                                    println!();
                                    let stop = Confirm::new("⚠️  Abort download and purge partial files?")
                                        .with_default(false)
                                        .with_render_config(config)
                                        .prompt();
                                    
                                    if let Ok(true) = stop {
                                        abort.store(true, std::sync::atomic::Ordering::SeqCst);
                                        aborted = true;
                                        break;
                                    } else {
                                        // Clear the confirm prompt and the added newline line (2 lines total)
                                        print!("\x1B[1A\x1B[2K\x1B[1A\r");
                                        let _ = stdout().flush();
                                    }
                                }
                            }
                        }

                        // 2. Check for download progress
                        match local_rx.try_recv() {
                            Ok(event) => {
                                if let DownloadEvent::Progress(_prog, current, total, speed, _eta) = event {
                                    let current_mb = current as f64 / 1_048_576.0;
                                    let total_mb = total as f64 / 1_048_576.0;
                                    let speed_mb = speed; // the downloader reports MB/s
                                    let elapsed_secs = start.elapsed().as_secs();
                                    let time_str = format!("[{:02}:{:02}]", elapsed_secs / 60, elapsed_secs % 60);

                                    let bar_width: usize = 40;
                                    let filled = if total > 0 { ((current as f64 / total as f64) * bar_width as f64) as usize } else { 0 };
                                    let empty = bar_width.saturating_sub(filled);
                                    let bar = format!("{}{}", "█".repeat(filled).cyan(), "░".repeat(empty).bright_black());

                                    print!("\x1B[2K\r  {} {} {} {:.2} / {:.2} MB | {:.2} MB/s", 
                                        time_str.bright_black(), 
                                        "DOWNLOADING".cyan().bold(),
                                        bar, 
                                        current_mb, total_mb, speed_mb
                                    );
                                    let _ = stdout().flush();
                                }
                            }
                            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                            _ => {}
                        }
                    }

                    if !aborted {
                        println!("\n\n  {} {}", "✔".green(), "Core matrix materialized locally.".white().bold());
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        rec.is_cached = true;
                    } else {
                        println!("\n\n  {} {}", "❌".red(), "Download aborted by user.".red().bold());
                        std::thread::sleep(std::time::Duration::from_millis(1200));
                    }
                    
                    // Cleanup progress lines (2 status + 1 newline + 2 padding = 5)
                    for _ in 0..5 { print!("\x1B[1A\x1B[2K\r"); }
                    let _ = stdout().flush();

                } else if choice.contains("DELETE") {
                    let confirm =
                        inquire::Confirm::new("⚠️  Are you sure you want to delete this model?")
                            .with_default(false)
                            .with_render_config(config)
                            .prompt();

                    match confirm {
                        Ok(true) => {
                            let _ = cluaiz_models::ModelDownloader::purge_model(&rec.manifest.category, &rec.manifest.id);
                            rec.is_cached = false;
                            println!("\n  {} {}", "🗑️".red(), "Model purged from local storage.".bold());
                            std::thread::sleep(std::time::Duration::from_millis(1200));
                            for _ in 0..3 { print!("\x1B[1A\x1B[2K\r"); }
                            let _ = stdout().flush();
                        }
                        _ => {
                            print!("\x1B[1A\x1B[2K\x1B[1A\x1B[2K\r");
                            let _ = stdout().flush();
                        }
                    }
                } else {
                    // 🔙 BACK Case
                    for _ in 0..lines_printed + 3 {
                        print!("\x1B[1A\x1B[2K\r");
                    }
                    let _ = stdout().flush();
                    return Ok(Some("BACK".to_string()));
                }
            }
            Err(_) => {
                for _ in 0..lines_printed + 1 {
                    print!("\x1B[1A\x1B[2K\r");
                }
                let _ = stdout().flush();
                return Ok(None);
            }
        }
    }
}
