//! What a model id means and which files to download: a catalog id, a Hugging Face repo
//! (`owner/name`, `hf://`, a URL), optionally with `:QUANT`. Shared by `cluaiz pull` / `cluaiz run`
//! and the API (`/api/pull`, CEP `models.download`): the caller's `ResolveUi` shows each step and
//! makes the choices (a prompt in the terminal, the first variant over the API).

use super::{package_bundler, parse_hf_repository, resolve_model_repo_with_fallback, DownloadEvent, HfTreeItem, HfVariant, HuggingFaceHub, ModelDownloader, RegistryResolveError};
use crate::models::registry::ModelCatalog;
use crate::models::taxonomy::quantization::UniversalQuantization;
use crate::models::types::manifest::ModelManifest;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

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

/// A step of the resolution, for the caller to show
pub enum ResolveStep<'a> {
    /// A short id is looked up in the online catalog
    Resolving(&'a str),
    RepoFound(&'a str),
    NotInCatalog,
    CatalogFailed,
    Scanning(&'a str),
    FetchingTree(&'a str),
    /// `:TAG` matched this variant
    TagMatched { tag: &'a str, variant: &'a HfVariant },
    TagNotFound { tag: &'a str, repo: &'a str },
    Repairing,
    FetchingMetadata,
}

impl std::fmt::Display for ResolveStep<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolving(id) => write!(f, "resolving {id}"),
            Self::RepoFound(repo) => write!(f, "found {repo}"),
            Self::NotInCatalog => write!(f, "no matching catalog id"),
            Self::CatalogFailed => write!(f, "catalog request failed"),
            Self::Scanning(repo) => write!(f, "scanning {repo}"),
            Self::FetchingTree(repo) => write!(f, "listing the files of {repo}"),
            Self::TagMatched { tag, variant } => write!(f, "tag :{tag} matched {} ({:.2} GB)", variant.variant_id, variant.size_gb),
            Self::TagNotFound { tag, repo } => write!(f, "tag :{tag} not found in {repo}"),
            Self::Repairing => write!(f, "existing package is incomplete; keeping valid files and repairing"),
            Self::FetchingMetadata => write!(f, "pulling manifest"),
        }
    }
}

/// How the caller shows steps and makes choices
pub trait ResolveUi: Sync {
    fn step(&self, step: ResolveStep<'_>);
    /// Index of the variant to download (`variants` is never empty)
    fn pick_variant(&self, repo: &str, variants: &[HfVariant]) -> Result<usize, String>;
    /// Files of the repository to download (`--all`)
    fn pick_files(&self, repo: &str, tree: &[HfTreeItem]) -> Result<Vec<String>, String>;
}

/// The answer when no one is there to choose: the matched `:TAG`, else the first variant;
/// a free pick of files is refused
pub struct FirstVariant<F: Fn(ResolveStep<'_>) + Sync>(pub F);

impl<F: Fn(ResolveStep<'_>) + Sync> ResolveUi for FirstVariant<F> {
    fn step(&self, step: ResolveStep<'_>) {
        (self.0)(step)
    }
    fn pick_variant(&self, _: &str, _: &[HfVariant]) -> Result<usize, String> {
        Ok(0)
    }
    fn pick_files(&self, _: &str, _: &[HfTreeItem]) -> Result<Vec<String>, String> {
        Err("Choosing files needs the terminal (`cluaiz pull --all`)".into())
    }
}

/// Finds what `model_id` means; `all` picks files from the whole repository
pub async fn resolve(model_id: &str, all: bool, ui: &dyn ResolveUi) -> Result<Resolved, String> {
    let roster = ModelCatalog::load_roster();

    // Optional explicit quantization/variant tag after a colon ('repo/model:IQ3_XXS', 'model:Q4_K_M')
    let (base_input, explicit_tag) = match model_id.rsplit_once(':') {
        Some((base, tag)) if !tag.is_empty() && !base.ends_with("http") && !base.ends_with("https") && !base.ends_with("hf") => {
            (base, Some(tag.to_string()))
        }
        _ => (model_id, None),
    };

    let mut target_hf_repo: Option<String> = None;
    let mut registry_failure: Option<String> = None;
    if !base_input.contains('/') && !base_input.starts_with("hf://") && !base_input.starts_with("https://") {
        let safe_id = model_id.replace(':', "-");
        let safe_base_id = base_input.replace(':', "-");
        let local_exists = roster.iter().any(|m| {
            (m.id.eq_ignore_ascii_case(model_id) || m.id.eq_ignore_ascii_case(base_input))
                && (ModelDownloader::model_dir(&m.category, &safe_id).join(&m.huggingface_filename).exists()
                    || ModelDownloader::model_dir(&m.category, &safe_base_id).join(&m.huggingface_filename).exists())
        });
        let locally_known = roster.iter().any(|model| model.id.eq_ignore_ascii_case(model_id));
        if !local_exists && !locally_known {
            // A short id not known here: the online catalog names its Hugging Face repo
            ui.step(ResolveStep::Resolving(base_input));
            let fallback = (base_input != model_id).then_some(base_input);
            match resolve_model_repo_with_fallback(model_id, fallback).await {
                Ok(Some(hf_repo)) => {
                    ui.step(ResolveStep::RepoFound(&hf_repo));
                    target_hf_repo = Some(hf_repo);
                }
                Ok(None) => {
                    ui.step(ResolveStep::NotInCatalog);
                    registry_failure = Some(RegistryResolveError::NotFound(model_id.to_string()).to_string());
                }
                Err(error) => {
                    ui.step(ResolveStep::CatalogFailed);
                    registry_failure = Some(error.to_string());
                }
            }
        }
    }

    let mut selected_variant_bundle: Option<HfVariant> = None;
    let manifest;
    let is_local;
    if base_input.contains('/') || target_hf_repo.is_some() {
        let resolved_id = target_hf_repo.unwrap_or_else(|| base_input.to_string());
        let repo_id = parse_hf_repository(&resolved_id).ok_or_else(|| RegistryResolveError::InvalidInput.to_string())?;
        ui.step(ResolveStep::Scanning(&repo_id));

        let selected_variant = if all {
            ui.step(ResolveStep::FetchingTree(&repo_id));
            let raw_tree = HuggingFaceHub::list_raw_tree(&repo_id).await?;
            if raw_tree.is_empty() {
                return Err(format!("No files found in repository '{repo_id}'"));
            }
            let selected_files = ui.pick_files(&repo_id, &raw_tree)?;
            if selected_files.is_empty() {
                return Err("No files selected for download.".into());
            }
            let primary_file = selected_files
                .iter()
                .find(|f| f.ends_with(".gguf") || f.ends_with(".onnx") || f.ends_with(".safetensors"))
                .cloned()
                .unwrap_or_else(|| selected_files[0].clone());
            let total_size_bytes: u64 = selected_files
                .iter()
                .filter_map(|f| raw_tree.iter().find(|item| &item.path == f).and_then(|i| i.size))
                .sum();
            let custom = HfVariant {
                revision: None,
                variant_id: "UNFILTERED CUSTOM BUNDLE".to_string(),
                format_type: if primary_file.ends_with(".onnx") { "onnx".to_string() } else { "gguf".to_string() },
                quant_tag: "CUSTOM".to_string(),
                primary_file: primary_file.clone(),
                all_files: selected_files,
                filename: primary_file,
                size_gb: total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            };
            package_bundler::complete_custom(&repo_id, &raw_tree, custom).await?
        } else {
            let variants = HuggingFaceHub::list_variants(&repo_id).await?;
            if variants.is_empty() {
                return Err(format!("No variants found in repository '{repo_id}'"));
            }
            let matched_from_tag = explicit_tag.as_ref().and_then(|tag| {
                let tag_upper = tag.to_uppercase();
                let clean_tag = tag_upper.replace(['-', '_'], "");
                variants
                    .iter()
                    .find(|v| {
                        v.quant_tag.to_uppercase() == tag_upper
                            || v.variant_id.to_uppercase().contains(&tag_upper)
                            || v.filename.to_uppercase().contains(&tag_upper)
                            || v.quant_tag.to_uppercase().replace(['-', '_'], "") == clean_tag
                            || v.filename.to_uppercase().replace(['-', '_'], "").contains(&clean_tag)
                    })
                    .cloned()
            });
            match (matched_from_tag, &explicit_tag) {
                (Some(matched), Some(tag)) => {
                    ui.step(ResolveStep::TagMatched { tag, variant: &matched });
                    matched
                }
                (_, tag) => {
                    if let Some(tag) = tag {
                        ui.step(ResolveStep::TagNotFound { tag, repo: &repo_id });
                    }
                    let index = ui.pick_variant(&repo_id, &variants)?;
                    variants.into_iter().nth(index).ok_or("Selected variant not found")?
                }
            }
        };

        // This exact variant may be installed already
        let selected_filename = &selected_variant.primary_file;
        if let Some(existing) = roster.iter().find(|m| {
            m.huggingface_repo.eq_ignore_ascii_case(&repo_id) && m.huggingface_filename.eq_ignore_ascii_case(selected_filename)
        }) {
            let primary = ModelDownloader::model_dir(&existing.category, &existing.id).join(&existing.huggingface_filename);
            if ModelDownloader::validate_package(&primary).is_ok() {
                return Ok(Resolved::AlreadyInstalled(existing.id.clone()));
            }
            ui.step(ResolveStep::Repairing);
        }

        ui.step(ResolveStep::FetchingMetadata);
        // The slot (chat, speech, transcription, embedding) comes from what the repository declares
        let (pipeline_tag, tags) = HuggingFaceHub::declared_tasks(&repo_id).await;
        let mut hf_manifest = HuggingFaceHub::build_manifest(&repo_id, &selected_variant, pipeline_tag.as_deref(), &tags);
        // The quant tag goes into the id for a flat vault folder: GLM-5.2-GGUF + UD-IQ1_S → GLM-5.2-GGUF-UD-IQ1_S
        let qt = &selected_variant.quant_tag;
        if !qt.is_empty() && qt != "DEFAULT" && !hf_manifest.id.to_uppercase().contains(&qt.to_uppercase()) {
            hf_manifest.id = format!("{}-{}", hf_manifest.id, qt);
        }
        // huggingface_filename is the flat basename of the primary file
        let primary = &selected_variant.primary_file;
        hf_manifest.huggingface_filename = primary.rsplit('/').next().unwrap_or(primary).to_string();
        manifest = hf_manifest;
        selected_variant_bundle = Some(selected_variant);
        is_local = false;
    } else if let Some(m) = roster.into_iter().find(|m| m.id.eq_ignore_ascii_case(model_id)) {
        is_local = ModelDownloader::model_dir(&m.category, &m.id.replace(':', "-")).join(&m.huggingface_filename).exists();
        manifest = m;
    } else {
        // The one catalog request already covered the remote catalog
        return Err(registry_failure.unwrap_or_else(|| RegistryResolveError::NotFound(model_id.to_string()).to_string()));
    }

    let model_file = ModelDownloader::model_dir(&manifest.category, &manifest.id).join(&manifest.huggingface_filename);
    if is_local && ModelDownloader::validate_package(&model_file).is_ok() {
        return Ok(Resolved::OnDisk(model_file));
    }
    if is_local && selected_variant_bundle.is_none() {
        let (repaired, files) = HuggingFaceHub::bundle_for_manifest(&manifest).await?;
        let quant = UniversalQuantization::extract_quant_tag(&repaired.huggingface_filename);
        return Ok(Resolved::Download(DownloadPlan { manifest: repaired, files, quant }));
    }
    let quant = selected_variant_bundle.as_ref().map(|v| v.quant_tag.clone()).unwrap_or_else(|| "DEFAULT".to_string());
    let files = selected_variant_bundle
        .map(|v| v.all_files)
        .unwrap_or_else(|| vec![manifest.huggingface_filename.clone()]);
    Ok(Resolved::Download(DownloadPlan { manifest, files, quant }))
}

/// What a pull with no one to ask ended in
pub enum Pulled {
    /// Downloaded and registered now: the weight file
    Downloaded(PathBuf),
    /// Nothing to download: the installed weight file or id
    OnDisk(PathBuf),
    AlreadyInstalled(String),
}

/// Resolve with `FirstVariant`, then download and register. Download progress goes to `tx`;
/// setting `abort` stops it.
pub async fn pull(
    model_id: &str,
    on_step: impl Fn(ResolveStep<'_>) + Send + Sync,
    tx: mpsc::Sender<DownloadEvent>,
    abort: Arc<AtomicBool>,
) -> Result<Pulled, String> {
    match resolve(model_id, false, &FirstVariant(on_step)).await? {
        Resolved::OnDisk(path) => Ok(Pulled::OnDisk(path)),
        Resolved::AlreadyInstalled(id) => Ok(Pulled::AlreadyInstalled(id)),
        Resolved::Download(plan) => ModelDownloader::install(&plan.manifest, &plan.files, tx, abort).await.map(Pulled::Downloaded),
    }
}
