pub mod asset_bundler;
pub mod auto_heal;
pub mod client;
pub mod downloader;
pub mod hf_hub;

pub use asset_bundler::AssetBundler;
pub use auto_heal::AutoHeal;
pub use client::{dispatch_model_telemetry, resolve_model_repo, RegistryClient};
pub use downloader::{DownloadEvent, FileDownloader};
pub use hf_hub::{HfTreeItem, HfVariant, HuggingFaceHub};

use crate::models::types::manifest::{ModelAsset, ModelManifest};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct ModelDownloader;

impl ModelDownloader {
    pub fn get_models_dir() -> PathBuf {
        cluaiz_core::EnvironmentManager::current()
            .ensure_models_dir()
            .unwrap_or_else(|_| cluaiz_core::EnvironmentManager::current().models_dir())
    }

    pub fn is_model_cached(category: &str, repo_id: &str, filename: &str) -> bool {
        Self::get_cached_path(category, repo_id, filename).is_some()
    }

    /// Vault folder of a model: `<models>/<category>/<last part of the id, ':' written as '-'>`.
    /// Every download, lookup and delete uses this one rule.
    pub fn model_dir(category: &str, model_id: &str) -> PathBuf {
        let name = model_id
            .split('/')
            .next_back()
            .unwrap_or(model_id)
            .replace(':', "-");
        crate::models::registry::ModelVault::resolve_category_dir(category).join(name)
    }

    pub fn get_cached_path(category: &str, repo_id: &str, filename: &str) -> Option<PathBuf> {
        let repo_path = Self::model_dir(category, repo_id);

        let file_basename = Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(filename);

        let weight_path = repo_path.join(file_basename);
        if weight_path.exists() {
            return Some(weight_path);
        }

        if let Ok(entries) = std::fs::read_dir(&repo_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gguf")
                    || path.extension().and_then(|s| s.to_str()) == Some("onnx")
                {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Synchronous block_on download wrapper
    pub fn download_gguf(
        category: &str,
        repo_id: &str,
        download_url: &str,
        filename: &str,
        assets: Vec<ModelAsset>,
        manifest: Option<ModelManifest>,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<PathBuf, String> {
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        rt.block_on(async {
            Self::download_gguf_async(
                category,
                repo_id,
                download_url,
                filename,
                assets,
                manifest,
                tx,
                abort,
            )
            .await
        })
    }

    /// Async download with progress channel, abort token, and manifest persistence
    pub async fn download_gguf_async(
        category: &str,
        repo_id: &str,
        download_url: &str,
        filename: &str,
        _assets: Vec<ModelAsset>,
        manifest: Option<ModelManifest>,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<PathBuf, String> {
        let dest_dir = Self::model_dir(category, repo_id);
        std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

        let file_basename = Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(filename);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .user_agent("cluaiz/1.0")
            .build()
            .map_err(|e| e.to_string())?;

        let dest_file = dest_dir.join(file_basename);
        if !dest_file.exists() {
            dispatch_model_telemetry(repo_id);
        }
        FileDownloader::download_single_file(&client, download_url, &dest_file, tx, abort).await?;

        // Save manifest if present
        if let Some(m) = manifest {
            if let Ok(json_str) = serde_json::to_string_pretty(&m) {
                let _ = std::fs::write(dest_dir.join("model_manifest.json"), json_str);
            }
        }

        Ok(dest_file)
    }

    /// Purges a model directory with 3 retry attempts
    pub fn purge_model(category: &str, repo_id: &str) -> Result<(), String> {
        let path = Self::model_dir(category, repo_id);
        if !path.exists() {
            return Err("Model directory not found".to_string());
        }
        Self::remove_folder(&path)?;
        let id = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        crate::models::registry::InstalledStateRegistry::unregister_model(&id)
    }

    /// Deletes a model folder (3 tries: Windows keeps a file locked for a moment after it is closed)
    fn remove_folder(path: &Path) -> Result<(), String> {
        for attempt in 1..=3 {
            match std::fs::remove_dir_all(path) {
                Ok(_) => return Ok(()),
                Err(_e) => {
                    std::thread::sleep(std::time::Duration::from_millis(500 * attempt as u64));
                }
            }
        }
        Err(format!(
            "could not delete {} (is the model loaded by a running server?)",
            path.display()
        ))
    }

    /// Deletes an installed model (its folder and its model_registry.json entry). `model_id` is the
    /// registry id `cluaiz list` shows.
    pub fn uninstall(model_id: &str) -> Result<PathBuf, String> {
        let reg = crate::models::registry::InstalledStateRegistry::load();
        let entry = reg
            .installed_models
            .get(model_id)
            .ok_or_else(|| format!("model '{}' is not installed (see `cluaiz list`)", model_id))?;
        let dir = PathBuf::from(&entry.local_dir);
        if dir.exists() {
            Self::remove_folder(&dir)?;
        }
        crate::models::registry::InstalledStateRegistry::unregister_model(model_id)?;
        Ok(dir)
    }

    /// Downloads `files` of a model into its vault folder and registers it in model_registry.json, as the
    /// old ModelManager did: files already on disk are kept, a failed download removes only the files this
    /// call created, and nothing is registered unless the primary weight file is there.
    /// Relocates graph-parent files together and preserves shared helper folders.
    /// Returns the primary weight file.
    pub async fn install(
        manifest: &ModelManifest,
        files: &[String],
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<PathBuf, String> {
        let dir = Self::model_dir(&manifest.category, &manifest.id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("could not create {}: {}", dir.display(), e))?;
        let primary_name = Path::new(&manifest.huggingface_filename)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| manifest.huggingface_filename.clone());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .user_agent("cluaiz/1.0")
            .build()
            .map_err(|e| e.to_string())?;

        let mut created: Vec<PathBuf> = Vec::new();
        let mapped = files
            .iter()
            .map(|path| AssetBundler::local_asset_path(path, &manifest.huggingface_filename))
            .collect::<Result<Vec<_>, _>>()?;
        let mut unique = std::collections::HashSet::new();
        if mapped.iter().any(|path| !unique.insert(path.clone())) {
            return Err("Model bundle contains conflicting local asset destinations".into());
        }
        for (idx, rel_path) in files.iter().enumerate() {
            let local_name = mapped[idx].to_string_lossy();
            let dest = dir.join(&mapped[idx]);
            if dest.exists() {
                println!(
                    "   ├─ [{}/{}] Already on disk: {}",
                    idx + 1,
                    files.len(),
                    local_name
                );
                continue;
            }
            if created.is_empty() {
                dispatch_model_telemetry(&manifest.id);
            }
            let url = if !manifest.huggingface_repo.is_empty() {
                format!(
                    "https://huggingface.co/{}/resolve/main/{}",
                    manifest.huggingface_repo, rel_path
                )
            } else {
                manifest.download_url.clone()
            };
            println!(
                "   ├─ [{}/{}] Downloading {}",
                idx + 1,
                files.len(),
                rel_path
            );
            if let Err(e) = FileDownloader::download_single_file(
                &client,
                &url,
                &dest,
                tx.clone(),
                abort.clone(),
            )
            .await
            {
                for path in &created {
                    let _ = std::fs::remove_file(path);
                }
                let _ = std::fs::remove_dir(&dir); // only when nothing else is in it
                return Err(format!("{}: {}", rel_path, e));
            }
            created.push(dest);
        }

        let primary = dir.join(&primary_name);
        if !primary.exists() {
            return Err(format!(
                "primary weight file '{}' is missing; the model was not registered",
                primary_name
            ));
        }
        if primary
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("onnx"))
        {
            Self::complete_onnx_assets(manifest, &primary, &client, tx.clone(), abort.clone())
                .await?;
        }
        if let Ok(json) = serde_json::to_string_pretty(manifest) {
            let _ = std::fs::write(dir.join("model_manifest.json"), json);
        }
        let entry = crate::models::registry::InstalledStateRegistry::entry_for_folder(
            &manifest.category,
            &dir,
            Some(&primary_name),
            &manifest.huggingface_repo,
        )
        .ok_or_else(|| format!("no weight file found in {}", dir.display()))?;
        if let Some(package) = entry
            .metadata
            .capabilities
            .as_ref()
            .filter(|p| !p.onnx_graphs.is_empty())
        {
            let missing = package.missing_onnx_assets();
            if !missing.is_empty() {
                return Err(format!(
                    "Required ONNX helper assets are missing: {}",
                    missing.join(", ")
                ));
            }
        }
        crate::models::registry::InstalledStateRegistry::register_model(entry)?;
        Ok(primary)
    }

    /// Fetch only genuinely referenced/required helpers from this selected
    /// repository. An absent asset is an error, never another model's config.
    pub async fn complete_onnx_assets(
        manifest: &ModelManifest,
        primary: &Path,
        client: &reqwest::Client,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let package = crate::capabilities::PackageCapabilities::inspect(primary)
            .map_err(|e| e.to_string())?;
        let missing = package.missing_onnx_assets();
        if missing.is_empty() {
            return Ok(());
        }
        if manifest.huggingface_repo.is_empty() {
            return Err(format!(
                "ONNX assets missing and no repository is configured: {}",
                missing.join(", ")
            ));
        }
        let tree = HuggingFaceHub::list_raw_tree(&manifest.huggingface_repo).await?;
        let dir = package
            .primary_file
            .parent()
            .ok_or("ONNX package directory missing")?;
        let graph_parent = Path::new(&manifest.huggingface_filename)
            .parent()
            .unwrap_or_else(|| Path::new(""));
        for asset in &missing {
            if abort.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("ONNX helper download cancelled".into());
            }
            let beside_graph = graph_parent
                .join(asset)
                .to_string_lossy()
                .replace('\\', "/");
            let external = package
                .onnx_graphs
                .iter()
                .any(|graph| graph.external_files.contains(asset));
            let candidates = if external {
                vec![beside_graph.as_str()]
            } else {
                vec![asset.as_str(), beside_graph.as_str()]
            };
            let source = candidates
                .into_iter()
                .find(|candidate| {
                    tree.iter().any(|item| {
                        item.path == *candidate && item.r#type.as_deref() == Some("file")
                    })
                })
                .ok_or_else(|| {
                    format!("Required ONNX asset '{asset}' is absent in selected repository")
                })?;
            let local = AssetBundler::local_asset_path(source, &manifest.huggingface_filename)?;
            if local != Path::new(asset) {
                return Err("ONNX helper destination does not match graph contract".into());
            }
            let url = format!(
                "https://huggingface.co/{}/resolve/main/{source}",
                manifest.huggingface_repo
            );
            FileDownloader::download_single_file(
                client,
                &url,
                &dir.join(local),
                tx.clone(),
                abort.clone(),
            )
            .await?;
        }
        let missing = crate::capabilities::PackageCapabilities::inspect(primary)
            .map_err(|e| e.to_string())?
            .missing_onnx_assets();
        if !missing.is_empty() {
            return Err(format!(
                "ONNX helper completion failed: {}",
                missing.join(", ")
            ));
        }
        Ok(())
    }

    /// Cleans up incomplete .part and .lock files in blobs directory
    pub fn cleanup_partial_download(category: &str, repo_id: &str) -> Result<(), String> {
        let blobs_path = Self::model_dir(category, repo_id).join("blobs");
        if let Ok(entries) = std::fs::read_dir(&blobs_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str());
                if ext == Some("part") || ext == Some("lock") {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        Ok(())
    }
}
