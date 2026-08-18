//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Autonomous Asset Auto-Heal & Fallback Recovery Engine
//! ═══════════════════════════════════════════════════════════════════════

use reqwest::Client;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub struct AutoHeal;

impl AutoHeal {
    /// Attempts to recover a missing configuration or tokenizer asset from fallback verified public repositories
    pub async fn auto_heal_missing_asset(
        target_dir: &Path,
        asset_name: &str,
        repo_fallbacks: &[&str],
    ) -> Result<(), String> {
        let dest_file = target_dir.join(asset_name);
        if dest_file.exists() {
            return Ok(());
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("cluaiz/1.0")
            .build()
            .map_err(|e| e.to_string())?;

        for &repo_id in repo_fallbacks {
            let url = format!("https://huggingface.co/{}/resolve/main/{}", repo_id, asset_name);
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    let mut file = tokio::fs::File::create(&dest_file).await.map_err(|e| e.to_string())?;
                    let mut stream = response.bytes_stream();
                    use futures_util::StreamExt;
                    while let Some(item) = stream.next().await {
                        let chunk = item.map_err(|e: reqwest::Error| e.to_string())?;
                        file.write_all(&chunk).await.map_err(|e: std::io::Error| e.to_string())?;
                    }
                    println!("🪄 [AUTO-HEAL] Recovered '{}' from fallback repository: {}", asset_name, repo_id);
                    return Ok(());
                }
            }
        }

        Ok(())
    }
}
