//! Download flow shared by `cluaiz pull` and `cluaiz run` (a model that is not installed yet):
//! resolve the id (`cluaiz_models::resolve`, shared with the API), show what will be downloaded,
//! ask, then download and register through `ModelDownloader::install`. This file is the terminal
//! side: the printed steps, the variant prompt and the `--all` file picker.

use color_eyre::Result;
use colored::Colorize;
use cluaiz_models::fetcher::{ResolveStep, ResolveUi};
use cluaiz_models::{HfTreeItem, HfVariant, ModelDownloader};
use std::io::{self, Write};
use std::path::PathBuf;

pub use cluaiz_models::fetcher::{DownloadPlan, Resolved};

/// Downloads run without questions (first variant, no confirm) when this is set, e.g. in scripts
pub const NON_INTERACTIVE_ENV: &str = "CLUAIZ_NONINTERACTIVE";

fn non_interactive() -> bool {
    std::env::var(NON_INTERACTIVE_ENV).is_ok()
}

/// The terminal's side of the resolution: printed steps, prompts
struct Terminal;

impl ResolveUi for Terminal {
    fn step(&self, step: ResolveStep<'_>) {
        match step {
            ResolveStep::Resolving(id) => {
                print!("  {} Resolving model ID '{}' via Registry...", "🔍".cyan(), id.bold());
                let _ = io::stdout().flush();
            }
            ResolveStep::RepoFound(repo) => println!(" -> Found HuggingFace Repo: '{}'", repo.yellow().bold()),
            ResolveStep::NotInCatalog => println!(" -> no matching catalogue ID"),
            ResolveStep::CatalogFailed => println!(" -> request failed"),
            ResolveStep::Scanning(repo) => println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo),
            ResolveStep::FetchingTree(repo) => println!("  {} Fetching full repository file tree for '{}'...", "🌐".cyan(), repo),
            ResolveStep::TagMatched { tag, variant } => println!(
                "  {} Direct Quantization Tag Matched: ':{}' -> {} ({:.2} GB)",
                "⚡".green().bold(),
                tag.yellow().bold(),
                variant.variant_id.cyan().bold(),
                variant.size_gb
            ),
            ResolveStep::TagNotFound { tag, repo } => println!(
                "  {} Quantization tag ':{}' not found in '{}'. Available variants listed below:",
                "⚠️".yellow(),
                tag.bold(),
                repo.cyan()
            ),
            ResolveStep::Repairing => println!("  Existing package is incomplete; keeping valid files and preparing repair."),
            ResolveStep::FetchingMetadata => println!("  {} Fetching precise metadata...", "📡".cyan()),
        }
    }

    fn pick_variant(&self, _: &str, variants: &[HfVariant]) -> Result<usize, String> {
        if non_interactive() {
            return Ok(0);
        }
        let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.variant_id, v.size_gb)).collect();
        inquire::Select::new("Select model variant bundle to download:", options)
            .with_page_size(12)
            .raw_prompt()
            .map(|selected| selected.index)
            .map_err(|e| format!("Selection cancelled: {}", e))
    }

    fn pick_files(&self, repo: &str, tree: &[HfTreeItem]) -> Result<Vec<String>, String> {
        run_directory_tree_picker(repo, tree).map_err(|e| e.to_string())
    }
}

/// Finds what `model_id` means: a catalog id, a Hugging Face repo (`owner/name`, `hf://`, a URL),
/// optionally with `:QUANT`. Asks for the variant when there is a choice.
pub async fn resolve(model_id: &str, all: bool) -> Result<Resolved> {
    println!("\n  {} [cluaiz] Initializing Kernel for '{}'...", "⚙️".yellow(), model_id.bold());
    cluaiz_models::fetcher::resolve(model_id, all, &Terminal).await.map_err(|e| color_eyre::eyre::eyre!(e))
}

/// Shows what will be downloaded and asks to go on. Memory placement is decided when loading;
/// download metadata is not a runtime memory requirement.
pub fn confirm(plan: &DownloadPlan) -> Result<()> {
    let manifest = &plan.manifest;
    let is_onnx = manifest.huggingface_filename.ends_with(".onnx") || manifest.architecture_type == "onnx";
    let format_display = if is_onnx { "ONNX" } else if manifest.huggingface_filename.ends_with(".gguf") { "GGUF" } else { "SafeTensors" };

    println!("\n  {} Model Metadata Summary:", "📋".cyan().bold());
    println!("    ├─ 🆔 ID: {}", manifest.id.cyan());
    println!("    ├─ 📦 Format: {}", format_display.cyan());
    println!("    ├─ 🏷️  Precision / Quant Tag: {}", plan.quant.magenta().bold());
    println!("    ├─ 🧠 Architecture: {}", manifest.architecture.yellow().bold());
    println!("    ├─ 📁 Category: {}", manifest.category.green());
    if !manifest.parameters.is_empty() && manifest.parameters != "Unknown" {
        println!("    ├─ 🧩 Parameters: {}", manifest.parameters.green());
    }
    println!("    ├─ 💾 Total Download Size: {:.2} GB", manifest.download_size_gb);
    let total = plan.files.len();
    println!("    └─ 📁 Files to Download ({} files):", total.to_string().cyan().bold());
    for (idx, f) in plan.files.iter().enumerate() {
        let connector = if idx == total - 1 { "       └─" } else { "       ├─" };
        let icon = if f.ends_with(".gguf") || f.ends_with(".onnx") || f.ends_with(".safetensors") {
            "⚖️ "
        } else if f.ends_with(".onnx_data") || f.ends_with(".data") {
            "📦"
        } else {
            "📄"
        };
        println!("{} {} {}", connector, icon, f.cyan());
    }

    if manifest.bit_depth > 0.0 && manifest.bit_depth < 3.0 {
        println!("\n  {} {:.2}-bit quantization (e.g. Q2_0 or BitNet): the backend may not support these weights.", "⚠️".yellow(), manifest.bit_depth);
    }

    let go_on = non_interactive() || inquire::Confirm::new("Proceed with model download?").with_default(true).prompt()?;
    if !go_on {
        return Err(color_eyre::eyre::eyre!("Download cancelled."));
    }
    Ok(())
}

/// Downloads the plan's files, registers the model, and returns its weight file.
pub async fn download(plan: &DownloadPlan) -> Result<PathBuf> {
    let (tx, _rx) = tokio::sync::mpsc::channel(100);
    let abort = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let path = ModelDownloader::install(&plan.manifest, &plan.files, tx, abort)
        .await
        .map_err(|e| color_eyre::eyre::eyre!("download failed: {}", e))?;
    let id = cluaiz_models::registry::InstalledStateRegistry::id_for_path(&path);
    println!("\n  {} Model downloaded and registered successfully!", "✅".green().bold());
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │ 🆔 Registered ID:  {}", id.cyan().bold());
    println!("  │ 📁 Vault Path:     {}", path.parent().map(|p| p.display().to_string()).unwrap_or_default().yellow());
    println!("  │ 🚀 Run Command:   {}", format!("cluaiz run {}", id).green().bold());
    println!("  └─────────────────────────────────────────────────────────────┘\n");
    Ok(path)
}

#[derive(Debug, Clone)]
enum TreeItemNode {
    ParentDir,
    Folder {
        name: String,
        full_path: String,
        selected_count: usize,
        total_count: usize,
    },
    File {
        name: String,
        full_path: String,
        size_bytes: u64,
        is_selected: bool,
    },
}

fn run_directory_tree_picker(repo_id: &str, raw_items: &[HfTreeItem]) -> Result<Vec<String>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType};
    use crossterm::cursor::{MoveTo, Hide, Show};
    use crossterm::execute;
    use std::collections::HashSet;

    let mut stdout = io::stdout();
    let mut selected_files: HashSet<String> = HashSet::new();
    let mut current_dir = String::new();
    let mut cursor_idx = 0;

    let all_files: Vec<(String, u64)> = raw_items.iter()
        .filter(|item| item.r#type.as_deref() != Some("directory"))
        .map(|item| (item.path.clone(), item.size.unwrap_or(0)))
        .collect();

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let result = (|| -> Result<Vec<String>> {
        loop {
            let mut folders: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
            let mut current_files: Vec<(String, String, u64)> = Vec::new();

            let prefix = if current_dir.is_empty() {
                "".to_string()
            } else if current_dir.ends_with('/') {
                current_dir.clone()
            } else {
                format!("{}/", current_dir)
            };

            for (path, size) in &all_files {
                if prefix.is_empty() || path.starts_with(&prefix) {
                    let rel = &path[prefix.len()..];
                    if let Some(slash_pos) = rel.find('/') {
                        let folder_name = &rel[..slash_pos];
                        let entry = folders.entry(folder_name.to_string()).or_insert((0, 0));
                        entry.1 += 1;
                        if selected_files.contains(path) {
                            entry.0 += 1;
                        }
                    } else {
                        current_files.push((rel.to_string(), path.clone(), *size));
                    }
                }
            }

            let mut nodes: Vec<TreeItemNode> = Vec::new();

            if !current_dir.is_empty() {
                nodes.push(TreeItemNode::ParentDir);
            }

            for (f_name, (sel_cnt, tot_cnt)) in folders {
                let full = if prefix.is_empty() {
                    f_name.clone()
                } else {
                    format!("{}{}", prefix, f_name)
                };
                nodes.push(TreeItemNode::Folder {
                    name: f_name,
                    full_path: full,
                    selected_count: sel_cnt,
                    total_count: tot_cnt,
                });
            }

            current_files.sort_by(|a, b| a.0.cmp(&b.0));
            for (f_name, full, size) in current_files {
                let is_sel = selected_files.contains(&full);
                nodes.push(TreeItemNode::File {
                    name: f_name,
                    full_path: full,
                    size_bytes: size,
                    is_selected: is_sel,
                });
            }

            if cursor_idx >= nodes.len() && !nodes.is_empty() {
                cursor_idx = nodes.len() - 1;
            }

            // Move cursor to top-left for zero-flicker drawing
            execute!(stdout, MoveTo(0, 0))?;

            let mut render_buf = String::new();

            let breadcrumb = if current_dir.is_empty() {
                format!("  📁 Root: {}", repo_id.bold().cyan())
            } else {
                format!("  📁 Location: {}", format!("{}/{}", repo_id, current_dir).bold().cyan())
            };

            render_buf.push_str(&format!("\x1B[K\r\n{}\x1B[K\r\n  {}\x1B[K\r\n", breadcrumb, "─".repeat(70).dimmed()));

            let total_selected_size: u64 = all_files.iter()
                .filter(|(p, _)| selected_files.contains(p))
                .map(|(_, s)| *s)
                .sum();
            let total_gb = total_selected_size as f64 / (1024.0 * 1024.0 * 1024.0);

            let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 40));
            // Header = 3 lines, Footer = 3 lines => 6 lines overhead + 1 safety
            let page_size = (term_height as usize).saturating_sub(7).max(5);
            let start_win = if cursor_idx >= page_size { cursor_idx - page_size + 1 } else { 0 };
            let end_win = (start_win + page_size).min(nodes.len());
            let visible_rows = end_win - start_win;

            let thumb_row = if nodes.len() > visible_rows {
                (cursor_idx * visible_rows) / nodes.len().max(1)
            } else {
                99999
            };

            for (row_idx, idx) in (start_win..end_win).enumerate() {
                let is_cursor = idx == cursor_idx;
                let pointer = if is_cursor { ">".green().bold().to_string() } else { " ".to_string() };
                let scroll_char = if nodes.len() > visible_rows {
                    if row_idx == thumb_row { "#".cyan().bold().to_string() } else { "|".dimmed().to_string() }
                } else {
                    " ".to_string()
                };

                match &nodes[idx] {
                    TreeItemNode::ParentDir => {
                        let parent_str = "[..] Parent Directory".cyan().bold();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {}\x1B[K\r\n", pointer, parent_str.on_black(), scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {}\x1B[K\r\n", pointer, parent_str, scroll_char));
                        }
                    }
                    TreeItemNode::Folder { name, selected_count, total_count, .. } => {
                        let check = if *selected_count == *total_count && *total_count > 0 {
                            "[✓]".green().bold()
                        } else if *selected_count > 0 {
                            "[-]".yellow().bold()
                        } else {
                            "[ ]".dimmed()
                        };
                        let folder_str = format!("{}/", name).cyan().bold();
                        let stats = format!("({}/{})", selected_count, total_count).dimmed();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, folder_str.on_black(), stats, scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, folder_str, stats, scroll_char));
                        }
                    }
                    TreeItemNode::File { name, size_bytes, is_selected, .. } => {
                        let check = if *is_selected { "[✓]".green().bold() } else { "[ ]".dimmed() };
                        let size_mb = *size_bytes as f64 / (1024.0 * 1024.0);
                        let size_str = format!("({:.1} MB)", size_mb).dimmed();
                        if is_cursor {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, name.yellow(), size_str, scroll_char));
                        } else {
                            render_buf.push_str(&format!("  {} {} {} {} {}\x1B[K\r\n", pointer, check, name, size_str, scroll_char));
                        }
                    }
                }
            }

            render_buf.push_str(&format!("  {}\x1B[K\r\n", "─".repeat(60).dimmed()));
            render_buf.push_str(&format!("  Selected: {} files ({:.2} GB)\x1B[K\r\n", selected_files.len().to_string().green().bold(), total_gb));
            render_buf.push_str(&format!("  {}  {}  {}  {}  {}  {}  {}\x1B[K\r\n", 
                "⌨".white().bold(), "↕:Up/Dn".yellow(), "↩:Enter".cyan(), "␣:Space".magenta(), "←:Back".blue(), "Y:Get".green().bold(), "Q:Exit".red().bold()));

            write!(stdout, "{}", render_buf)?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Up => {
                            if cursor_idx > 0 {
                                cursor_idx -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if cursor_idx + 1 < nodes.len() {
                                cursor_idx += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if cursor_idx < nodes.len() {
                                match &nodes[cursor_idx] {
                                    TreeItemNode::ParentDir => {
                                        if let Some(last_slash) = current_dir.rfind('/') {
                                            current_dir = current_dir[..last_slash].to_string();
                                        } else {
                                            current_dir.clear();
                                        }
                                        cursor_idx = 0;
                                        execute!(stdout, Clear(ClearType::All))?;
                                    }
                                    TreeItemNode::Folder { full_path, .. } => {
                                        current_dir = full_path.clone();
                                        cursor_idx = 0;
                                        execute!(stdout, Clear(ClearType::All))?;
                                    }
                                    TreeItemNode::File { full_path, is_selected, .. } => {
                                        if *is_selected {
                                            selected_files.remove(full_path);
                                        } else {
                                            selected_files.insert(full_path.clone());
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Left | KeyCode::Esc => {
                            if !current_dir.is_empty() {
                                if let Some(last_slash) = current_dir.rfind('/') {
                                    current_dir = current_dir[..last_slash].to_string();
                                } else {
                                    current_dir.clear();
                                }
                                cursor_idx = 0;
                                execute!(stdout, Clear(ClearType::All))?;
                            } else {
                                return Err(color_eyre::eyre::eyre!("Selection cancelled by user"));
                            }
                        }
                        KeyCode::Char(' ') => {
                            if cursor_idx < nodes.len() {
                                match &nodes[cursor_idx] {
                                    TreeItemNode::ParentDir => {}
                                    TreeItemNode::Folder { full_path, .. } => {
                                        let folder_prefix = format!("{}/", full_path);
                                        let child_files: Vec<String> = all_files.iter()
                                            .filter(|(p, _)| p.starts_with(&folder_prefix))
                                            .map(|(p, _)| p.clone())
                                            .collect();
                                        
                                        let all_sel = child_files.iter().all(|f| selected_files.contains(f));
                                        if all_sel {
                                            for f in child_files {
                                                selected_files.remove(&f);
                                            }
                                        } else {
                                            for f in child_files {
                                                selected_files.insert(f);
                                            }
                                        }
                                    }
                                    TreeItemNode::File { full_path, is_selected, .. } => {
                                        if *is_selected {
                                            selected_files.remove(full_path);
                                        } else {
                                            selected_files.insert(full_path.clone());
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Tab => {
                            if selected_files.is_empty() {
                                continue;
                            }
                            let mut sorted: Vec<String> = selected_files.into_iter().collect();
                            sorted.sort();
                            return Ok(sorted);
                        }
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            return Err(color_eyre::eyre::eyre!("Selection cancelled by user"));
                        }
                        KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            return Err(color_eyre::eyre::eyre!("Aborted by user"));
                        }
                        _ => {}
                    }
                }
            }
        }
    })();

    let _ = execute!(stdout, LeaveAlternateScreen, Show);
    let _ = disable_raw_mode();
    result
}
