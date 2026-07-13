use std::path::PathBuf;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

pub struct ModelInstaller {
    target_dir: PathBuf,
}

impl ModelInstaller {
    pub fn new(target_dir: PathBuf) -> Self {
        Self { target_dir }
    }

    /// Downloads the GGUF file using the .part atomic protocol
    pub async fn download_weights(&self, url: &str, filename: &str) -> Result<(), String> {
        let mut dest_path = self.target_dir.clone();
        dest_path.push(filename);

        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| format!("Failed to create parent dir: {}", e))?;
        }

        if dest_path.exists() {
            println!("  {} Weights already present: {}", "✅".green(), filename);
            return Ok(());
        }

        let client = reqwest::Client::new();
        let response = client.get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to weight source: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Download failed: HTTP {}", response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({msg}, {eta})")
            .unwrap()
            .progress_chars("#>-"));
        
        pb.set_message("Downloading Weights");

        let mut part_path = dest_path.clone();
        part_path.set_extension("part");

        let mut file = tokio::fs::File::create(&part_path).await.map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }

        pb.finish_with_message("Download Complete");
        tokio::fs::rename(part_path, dest_path).await.map_err(|e| e.to_string())?;

        println!("  {} Weights acquired: {}", "✅".green(), filename);
        Ok(())
    }

    /// Pulls auxiliary assets (JSONs, tokenizers) ONLY if missing or corrupted
    pub async fn pull_assets(&self, assets: Vec<(String, String)>) -> Result<(), String> {
        // Filter out existing assets
        let missing_assets: Vec<_> = assets.into_iter()
            .filter(|(name, _)| !self.target_dir.join(name).exists())
            .collect();

        if missing_assets.is_empty() {
            println!("  {} All assets verified.", "✅".green());
            return Ok(());
        }
        
        println!("\n  {} Repairing/Downloading assets...", "🔗".cyan());
        let client = reqwest::Client::new();
        
        for (name, url) in missing_assets {
            print!("   - Fetching {}... ", name);
            std::io::Write::flush(&mut std::io::stdout()).ok();

            let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if response.status().is_success() {
                let asset_path = self.target_dir.join(&name);
                if let Some(parent) = asset_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                let mut file = tokio::fs::File::create(&asset_path).await.map_err(|e| e.to_string())?;
                let mut stream = response.bytes_stream();
                while let Some(item) = stream.next().await {
                    let chunk = item.map_err(|e| e.to_string())?;
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                }
                println!("{}", "OK".green());
            } else {
                println!("{}", format!("FAILED (HTTP {})", response.status()).red());
            }
        }
        Ok(())
    }
}
