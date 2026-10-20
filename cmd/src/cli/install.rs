//! Download flow shared by `cluaiz pull` and `cluaiz run` (a model that is not installed yet):
//! resolve the id (registry, Hugging Face repo, `:QUANT` tag, `--all` file picker), show what will be
//! downloaded, ask, then download and register through `ModelDownloader::install`.

use color_eyre::Result;
use colored::Colorize;
use cluaiz_models::{CoreRoster, HardwareAuditor, HealthStatus, HfTreeItem, HfVariant, HuggingFaceHub, ModelDownloader, ModelManifest, resolve_model_repo};
use std::io::{self, Write};
use std::path::PathBuf;

/// Downloads run without questions (first variant, no confirm) when this is set, e.g. in scripts
pub const NON_INTERACTIVE_ENV: &str = "CLUAIZ_NONINTERACTIVE";

fn non_interactive() -> bool {
    std::env::var(NON_INTERACTIVE_ENV).is_ok()
}

/// What a model id turned out to be
pub enum Resolved {
    /// The weight file is on disk already
    OnDisk(PathBuf),
    /// The chosen variant is installed under this id
    AlreadyInstalled(String),
    Download(DownloadPlan),
}

pub struct DownloadPlan {
    pub manifest: ModelManifest,
    /// Repository paths to download (all files of the chosen variant)
    pub files: Vec<String>,
    pub quant: String,
}

/// Finds what `model_id` means: a catalog id, a Hugging Face repo (`owner/name`, `hf://`, a URL),
/// optionally with `:QUANT`. Asks for the variant when there is a choice.
pub async fn resolve(model_id: &str, all: bool) -> Result<Resolved> {
    // 🎨 Display the Logo
    let logo = crate::assets::logos::logo_gallery::LOGO_VARIANTS[9];
    println!("{}", logo.cyan());

    println!("\n  {} [cluaiz] Initializing Kernel for '{}'...", "⚙️".yellow(), model_id.bold());

    let mut manifest: Option<ModelManifest>;
    let mut selected_variant_bundle: Option<HfVariant> = None;
    let mut is_local = false;

    let roster = CoreRoster::load_roster();

    // Extract optional explicit quantization/variant tag if specified with colon (e.g. 'repo/model:IQ3_XXS' or 'model:Q4_K_M')
    let (base_input, explicit_tag) = if let Some((base, tag)) = model_id.rsplit_once(':') {
        if !tag.is_empty() && !base.ends_with("http") && !base.ends_with("https") && !base.ends_with("hf") {
            (base, Some(tag.to_string()))
        } else {
            (model_id, None)
        }
    } else {
        (model_id, None)
    };

    let mut target_hf_repo: Option<String> = None;
    if !base_input.contains('/') && !base_input.starts_with("hf://") && !base_input.starts_with("https://") {
        // Check if model already exists locally
        let safe_id = model_id.replace(':', "-");
        let safe_base_id = base_input.replace(':', "-");
        let local_exists = roster.iter().any(|m| {
            if m.id.to_lowercase() == model_id.to_lowercase() || m.id.to_lowercase() == base_input.to_lowercase() {
                let model_path = ModelDownloader::model_dir(&m.category, &safe_id);
                let base_path = ModelDownloader::model_dir(&m.category, &safe_base_id);
                model_path.join(&m.huggingface_filename).exists() || base_path.join(&m.huggingface_filename).exists()
            } else {
                false
            }
        });

        if !local_exists {
            // Short ID given and not present locally -> Resolve upstream Hugging Face repo from Registry
            print!("  {} Resolving model ID '{}' via Registry...", "🔍".cyan(), base_input.bold());
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let resolved_res = match resolve_model_repo(model_id).await {
                Ok(Some(repo)) => Ok(Some(repo)),
                _ => resolve_model_repo(base_input).await,
            };

            if let Ok(Some(hf_repo)) = resolved_res {
                println!(" -> Found HuggingFace Repo: '{}'", hf_repo.yellow().bold());
                target_hf_repo = Some(hf_repo);
            } else {
                println!(" (using local/external catalog)");
            }
        }
    }

    if base_input.contains('/') || target_hf_repo.is_some() {
        // HUGGINGFACE REQUEST (EXPLICIT OR RESOLVED)
        let mut resolved_id = target_hf_repo.unwrap_or_else(|| base_input.to_string());
        if !resolved_id.starts_with("hf://") && !resolved_id.starts_with("https://") {
            resolved_id = format!("hf://{}", resolved_id);
        }
        
        let repo_id = resolved_id.replace("hf://", "").replace("https://huggingface.co/", "");
        let repo_id = if repo_id.ends_with('/') { repo_id[..repo_id.len()-1].to_string() } else { repo_id };
        
        println!("  {} Scanning HuggingFace Hub for '{}'...", "🔍".cyan(), repo_id);
        
        let selected_variant = if all {
            println!("  {} Fetching full repository file tree for '{}'...", "🌐".cyan(), repo_id);
            let raw_tree = HuggingFaceHub::list_raw_tree(&repo_id).await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;

            if raw_tree.is_empty() {
                return Err(color_eyre::eyre::eyre!("No files found in repository '{}'", repo_id));
            }

            let selected_files = run_directory_tree_picker(&repo_id, &raw_tree)?;

            if selected_files.is_empty() {
                return Err(color_eyre::eyre::eyre!("No files selected for download."));
            }

            let primary_file = selected_files.iter()
                .find(|f| f.ends_with(".gguf") || f.ends_with(".onnx") || f.ends_with(".safetensors"))
                .cloned()
                .unwrap_or_else(|| selected_files[0].clone());

            let total_size_bytes: u64 = selected_files.iter()
                .filter_map(|f| raw_tree.iter().find(|item| &item.path == f).and_then(|i| i.size))
                .sum();
            let size_gb = total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

            let custom = HfVariant {
                revision: None,
                variant_id: "UNFILTERED CUSTOM BUNDLE".to_string(),
                format_type: if primary_file.ends_with(".onnx") { "onnx".to_string() } else { "gguf".to_string() },
                quant_tag: "CUSTOM".to_string(),
                primary_file: primary_file.clone(),
                all_files: selected_files,
                filename: primary_file,
                size_gb,
            };
            cluaiz_models::fetcher::package_bundler::complete_custom(&repo_id, &raw_tree, custom).await
                .map_err(|e| color_eyre::eyre::eyre!(e))?
        } else {
            let variants = HuggingFaceHub::list_variants(&repo_id).await
                .map_err(|e| color_eyre::eyre::eyre!(e))?;

            if variants.is_empty() {
                return Err(color_eyre::eyre::eyre!("No variants found in repository '{}'", repo_id));
            }

            // ⚡ If an explicit quantization tag was given (e.g. :IQ3_XXS or :Q4_K_M), auto-select it directly!
            let matched_from_tag = if let Some(ref tag) = explicit_tag {
                let tag_upper = tag.to_uppercase();
                let clean_tag = tag_upper.replace(['-', '_'], "");
                
                variants.iter().find(|v| {
                    v.quant_tag.to_uppercase() == tag_upper
                        || v.variant_id.to_uppercase().contains(&tag_upper)
                        || v.filename.to_uppercase().contains(&tag_upper)
                        || v.quant_tag.to_uppercase().replace(['-', '_'], "") == clean_tag
                        || v.filename.to_uppercase().replace(['-', '_'], "").contains(&clean_tag)
                }).cloned()
            } else {
                None
            };

            if let Some(matched) = matched_from_tag {
                println!(
                    "  {} Direct Quantization Tag Matched: ':{}' -> {} ({:.2} GB)",
                    "⚡".green().bold(),
                    explicit_tag.as_ref().unwrap().yellow().bold(),
                    matched.variant_id.cyan().bold(),
                    matched.size_gb
                );
                matched
            } else {
                if let Some(ref tag) = explicit_tag {
                    println!(
                        "  {} Quantization tag ':{}' not found in '{}'. Available variants listed below:",
                        "⚠️".yellow(),
                        tag.bold(),
                        repo_id.cyan()
                    );
                }

                if non_interactive() {
                    variants.first().ok_or_else(|| color_eyre::eyre::eyre!("No variants found"))?.clone()
                } else {
                    let options: Vec<String> = variants.iter().map(|v| format!("{} ({:.2} GB)", v.variant_id, v.size_gb)).collect();
                    let selected_option = inquire::Select::new("Select model variant bundle to download:", options)
                        .with_page_size(12)
                        .raw_prompt()
                        .map_err(|e| color_eyre::eyre::eyre!("Selection cancelled: {}", e))?;
                    
                    variants.into_iter().nth(selected_option.index)
                        .ok_or_else(|| color_eyre::eyre::eyre!("Selected variant not found"))?
                }
            }
        };

        let selected_filename = selected_variant.primary_file.clone();
        
        // 🚀 Check if this specific variant is already in the local roster!
        if let Some(existing) = roster.iter().find(|m| m.huggingface_repo.to_lowercase() == repo_id.to_lowercase() && m.huggingface_filename.to_lowercase() == selected_filename.to_lowercase()) {
            let primary = ModelDownloader::model_dir(&existing.category, &existing.id).join(&existing.huggingface_filename);
            if ModelDownloader::validate_package(&primary).is_ok() {
                return Ok(Resolved::AlreadyInstalled(existing.id.clone()));
            }
            println!("  Existing package is incomplete; keeping valid files and preparing repair.");
        }

        println!("  {} Fetching precise metadata...", "📡".cyan());
        let hf_manifest = HuggingFaceHub::build_manifest(&repo_id, &selected_variant, None);
            
        manifest = Some(hf_manifest);
        selected_variant_bundle = Some(selected_variant);
        is_local = false;

        // Append quant tag to manifest ID for flat vault folder naming
        // e.g. GLM-5.2-GGUF + UD-IQ1_S → GLM-5.2-GGUF-UD-IQ1_S (flat, not nested)
        if let (Some(ref mut m), Some(ref bundle)) = (&mut manifest, &selected_variant_bundle) {
            let qt = bundle.quant_tag.clone();
            if !qt.is_empty() && qt != "DEFAULT" {
                let qt_upper = qt.to_uppercase();
                let id_upper = m.id.to_uppercase();
                if !id_upper.contains(&qt_upper) {
                    m.id = format!("{}-{}", m.id, qt);
                }
            }
            // Set huggingface_filename to the flat basename of the primary file
            let flat_name = bundle.primary_file.rsplit('/').next().unwrap_or(&bundle.primary_file).to_string();
            m.huggingface_filename = flat_name;
        }
    } else {
        // REGISTRY OR LOCAL ID REQUEST
        if let Some(m) = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase()) {
            let safe_id = m.id.replace(':', "-");
            let model_path = ModelDownloader::model_dir(&m.category, &safe_id);
            let model_file = model_path.join(&m.huggingface_filename);

            if model_file.exists() {
                manifest = Some(m);
                is_local = true;
            } else {
                manifest = Some(m);
                is_local = false;
            }
        } else {
            // Not in local vault, fetch from external registry
            println!("  {} Model missing in local vault. Synchronizing with Registry...", "🌐".yellow());
            let remote_models = CoreRoster::fetch_external_registry(None).await.map_err(|e| color_eyre::eyre::eyre!(e))?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }
    }

    let manifest = manifest.ok_or_else(|| color_eyre::eyre::eyre!("ID '{}' not found in any registry.", model_id))?;
    let model_file = ModelDownloader::model_dir(&manifest.category, &manifest.id).join(&manifest.huggingface_filename);
    if is_local && ModelDownloader::validate_package(&model_file).is_ok() {
        return Ok(Resolved::OnDisk(model_file));
    }
    if is_local && selected_variant_bundle.is_none() {
        let (repaired, files) = HuggingFaceHub::bundle_for_manifest(&manifest).await
            .map_err(|e| color_eyre::eyre::eyre!(e))?;
        let quant = cluaiz_models::taxonomy::quantization::UniversalQuantization::extract_quant_tag(&repaired.huggingface_filename);
        return Ok(Resolved::Download(DownloadPlan { manifest: repaired, files, quant }));
    }
    let quant = selected_variant_bundle.as_ref().map(|v| v.quant_tag.clone()).unwrap_or_else(|| "DEFAULT".to_string());
    let files = selected_variant_bundle
        .map(|v| v.all_files)
        .unwrap_or_else(|| vec![manifest.huggingface_filename.clone()]);
    Ok(Resolved::Download(DownloadPlan { manifest, files, quant }))
}

/// Shows what will be downloaded and asks to go on. The hardware check only warns: the planner
/// decides the placement (GPU, RAM split, disk streaming) when the model is loaded.
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
    println!("    ├─ 🧮 RAM Projected: {:.2} GB", manifest.ram_required_gb);
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
    if HardwareAuditor.audit_performance(manifest.ram_required_gb as f32, manifest.requires_gpu) == HealthStatus::Disabled {
        println!("  {} This model needs more memory than this machine has; it may load very slowly or not at all.", "⚠️".yellow());
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
    let id = crate::cli::models::id_for_path(&path);
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
