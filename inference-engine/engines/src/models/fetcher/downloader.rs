//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Atomic & Resumable File Downloader
//! ═══════════════════════════════════════════════════════════════════════

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use futures_util::StreamExt;
use reqwest::Client;

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(f32, u64, u64, f64, u64),
    Complete(String),
    Error(String, String),
    PurgeComplete(String),
    PurgeError(String, String),
}

pub struct FileDownloader;

impl FileDownloader {
    /// Atomically downloads a single remote URL to a local destination file path using .part protocol
    pub async fn download_single_file(
        client: &Client,
        url: &str,
        dest_path: &Path,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>,
    ) -> Result<(), String> {
        if dest_path.exists() {
            let _ = tx.send(DownloadEvent::Complete(dest_path.to_string_lossy().to_string())).await;
            return Ok(());
        }

        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        let mut req = client.get(url);
        if let Some(token) = crate::models::fetcher::hf_hub::HuggingFaceHub::resolve_hf_token() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        let response = req
            .send()
            .await
            .map_err(|e| format!("Failed to connect to source {}: {}", url, e))?;

        if !response.status().is_success() {
            let status = response.status();
            let err = if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
                format!(
                    "🔒 [Gated Model Download Blocked - HTTP {}]\nThis Hugging Face repository requires an authorized token.\n1. Accept license terms on Hugging Face.\n2. Set your token:\n   PowerShell:   $env:HF_TOKEN=\"hf_your_token\"\n   Windows CMD:  set HF_TOKEN=hf_your_token\n   Linux / Mac:  export HF_TOKEN=\"hf_your_token\"",
                    status
                )
            } else {
                format!("Download failed: HTTP {}", status)
            };
            let _ = tx.send(DownloadEvent::Error(dest_path.to_string_lossy().to_string(), err.clone())).await;
            return Err(err);
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut part_path = dest_path.to_path_buf();
        part_path.set_extension("part");

        let mut file = tokio::fs::File::create(&part_path)
            .await
            .map_err(|e| e.to_string())?;

        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let start_time = std::time::Instant::now();

        while let Some(item) = stream.next().await {
            if abort.load(Ordering::Relaxed) {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err("Download aborted by user.".to_string());
            }

            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (downloaded as f64 / 1024.0 / 1024.0) / elapsed
            } else {
                0.0
            };

            let percent = if total_size > 0 {
                (downloaded as f32 / total_size as f32) * 100.0
            } else {
                0.0
            };

            let _ = tx.send(DownloadEvent::Progress(percent, downloaded, total_size, speed, 0)).await;
        }

        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);

        tokio::fs::rename(&part_path, dest_path)
            .await
            .map_err(|e| format!("Failed to finalize downloaded file: {}", e))?;

        let _ = tx.send(DownloadEvent::Complete(dest_path.to_string_lossy().to_string())).await;
        Ok(())
    }
}
