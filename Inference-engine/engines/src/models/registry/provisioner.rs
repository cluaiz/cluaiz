use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use tracing::{info, warn};
use reqwest;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;

#[derive(Debug)]
pub struct Provisioner;

impl Provisioner {
    /// Ensures that the required assets (like tokenizer.json) exist in the model directory.
    /// If missing, it attempts to discover and download them from Hugging Face.
    pub async fn ensure_assets(
        model_dir: &Path,
        primary_repo: &str,
        fallback_repo: Option<&str>,
        filename: &str,
    ) -> Result<PathBuf> {
        let target_path = model_dir.join(filename);

        // 1. Check local existence
        if target_path.exists() {
            info!("Asset found locally: {:?}", target_path);
            return Ok(target_path);
        }

        info!("Asset missing locally. Starting autonomous discovery for: {}", filename);

        // 2. Tiered Discovery: Try Primary Repo then Fallback Repo
        let mut repos = vec![primary_repo.to_string()];
        if let Some(fallback) = fallback_repo {
            repos.push(fallback.to_string());
        }

        for repo in repos {
            match Self::download_from_hf(&repo, filename, &target_path).await {
                Ok(_) => {
                    info!("Successfully provisioned {} from repo: {}", filename, repo);
                    return Ok(target_path);
                }
                Err(e) => {
                    warn!("Failed to discover {} in repo {}: {}", filename, repo, e);
                }
            }
        }

        Err(anyhow!(
            "Cluaiz Registry Alert: Failed to provision required asset '{}' from all sources.",
            filename
        ))
    }

    async fn download_from_hf(repo: &str, filename: &str, target_path: &PathBuf) -> Result<()> {
        // We try the common 'main' branch first
        let url = format!("https://huggingface.co/{}/raw/main/{}", repo, filename);
        
        info!("Attempting download from: {}", url);
        
        let response = reqwest::get(&url).await?;

        if response.status().is_success() {
            let mut file = tokio::fs::File::create(target_path).await?;
            let mut stream = response.bytes_stream();
            while let Some(item) = stream.next().await {
                let chunk = item?;
                file.write_all(&chunk).await?;
            }
            return Ok(());
        }

        // Potential fallback for 'master' branch if 'main' fails
        if response.status().as_u16() == 404 {
             let master_url = format!("https://huggingface.co/{}/raw/master/{}", repo, filename);
             info!("Retrying on master branch: {}", master_url);
             let master_response = reqwest::get(&master_url).await?;
             if master_response.status().is_success() {
                 let mut file = tokio::fs::File::create(target_path).await?;
                 let mut stream = master_response.bytes_stream();
                 while let Some(item) = stream.next().await {
                     let chunk = item?;
                     file.write_all(&chunk).await?;
                 }
                 return Ok(());
             }
        }

        Err(anyhow!("HTTP Error: {}", response.status()))
    }
}
