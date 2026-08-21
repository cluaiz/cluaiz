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
        cluaiz_shared::environment::EnvironmentManager::current()
            .ensure_models_dir()
            .unwrap_or_else(|_| cluaiz_shared::environment::EnvironmentManager::current().models_dir())
    }

    pub fn is_model_cached(category: &str, repo_id: &str, filename: &str) -> bool {
        Self::get_cached_path(category, repo_id, filename).is_some()
    }

    pub fn get_cached_path(category: &str, repo_id: &str, filename: &str) -> Option<PathBuf> {
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id).replace(':', "-");
        let models_dir = Self::get_models_dir();
        let repo_path = models_dir.join(category).join(model_name);

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
            Self::download_gguf_async(category, repo_id, download_url, filename, assets, manifest, tx, abort).await
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
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let dest_dir = Self::get_models_dir().join(category).join(model_name);
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
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let path = Self::get_models_dir().join(category).join(model_name);
        if !path.exists() {
            return Err("Model directory not found".to_string());
        }
        for attempt in 1..=3 {
            match std::fs::remove_dir_all(&path) {
                Ok(_) => return Ok(()),
                Err(_e) => {
                    std::thread::sleep(std::time::Duration::from_millis(500 * attempt as u64));
                }
            }
        }
        Err("Purge failed after 3 attempts.".to_string())
    }

    /// Cleans up incomplete .part and .lock files in blobs directory
    pub fn cleanup_partial_download(category: &str, repo_id: &str) -> Result<(), String> {
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let blobs_path = Self::get_models_dir().join(category).join(model_name).join("blobs");
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
