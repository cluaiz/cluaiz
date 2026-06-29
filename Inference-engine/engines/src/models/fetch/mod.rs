use std::path::PathBuf;
use tracing::info;
use tokio::sync::mpsc;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use crate::models::registry::ModelManifest;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Progress(f32, u64, u64, f64, u64),
    Complete(String),
    Error(String, String),
    PurgeComplete(String),
    PurgeError(String, String),
}

pub struct ModelDownloader;

impl ModelDownloader {
    fn get_models_dir() -> PathBuf {
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
        
        let file_basename = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(filename);
        
        // 1. Check for main weight file
        let weight_path = repo_path.join(file_basename);
        if weight_path.exists() { return Some(weight_path); }
        
        // 2. Fallback: Search for any GGUF in the directory
        if let Ok(entries) = std::fs::read_dir(&repo_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                    return Some(path);
                }
            }
        }
        None
    }

    /// 🌐 NATIVE DOWNLOAD: Includes 'abort' signal, multi-asset support, and manifest generation.
    pub async fn download_gguf_async(
        category: &str,
        repo_id: &str,
        download_url: &str,
        filename: &str,
        _assets: Vec<crate::models::registry::ModelAsset>,
        manifest: Option<ModelManifest>,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>
    ) -> Result<PathBuf, String> {
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let dest_dir = Self::get_models_dir().join(category).join(model_name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

        let file_basename = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(filename);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3600))
            .user_agent("cluaiz/1.0")
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(reqwest::header::REFERER, "https://huggingface.co/".parse().unwrap_or(reqwest::header::HeaderValue::from_static("https://huggingface.co/")));
                headers.insert(reqwest::header::ACCEPT, "*/*".parse().unwrap_or(reqwest::header::HeaderValue::from_static("*/*")));
                headers
            })
            .build()
            .map_err(|e| e.to_string())?;

        // 1. Download the main weights (The ONLY file downloaded)
        Self::download_single_file(&client, download_url, &dest_dir.join(file_basename), tx.clone(), abort.clone()).await?;

        // 2. ✅ Save model_manifest.json — makes the folder fully self-contained & portable
        let weight_path = dest_dir.join(file_basename);
        if let Some(m) = manifest {
            let manifest_path = dest_dir.join("model_manifest.json");
            if let Ok(json) = serde_json::to_string_pretty(&m) {
                let _ = std::fs::write(&manifest_path, json);
            }
            
            // 🧬 DNA HANDSHAKE: Generate structural_dna.json with Binary Trace
            let _ = Self::generate_cluaiz_dna(&m, &dest_dir, &weight_path);
        }

        Ok(weight_path)
    }

    /// 🧬 DNA GENERATOR: Creates the structural backbone for the engine's loader by probing the binary.
    pub fn generate_cluaiz_dna(manifest: &ModelManifest, dest_dir: &std::path::Path, weight_path: &std::path::Path) -> Result<(), String> {
        info!("🧬 [DNA] Generating Cluaiz architectural backbone for '{}'", manifest.id);
        
        let mut signature = cluaiz_shared::KernelSignature::default();
        signature.is_multimodal = manifest.has_vision;
        if manifest.expert_count.is_some() {
            signature.has_experts = true;
        }

        let bit_val = manifest.bit_depth;

        let mut dna = crate::models::registry::StructuralDNA::create_skeleton(
            manifest.id.clone(),
            manifest.has_vision,
            manifest.expert_count,
            bit_val,
            &manifest.context_window,
        );

        // Add extra dynamic attributes
        dna.dynamic_attributes.insert("bit_depth".to_string(), bit_val.to_string());
        dna.dynamic_attributes.insert("parameters".to_string(), manifest.parameters.clone());
        dna.dynamic_attributes.insert("training_tokens".to_string(), manifest.training_tokens.clone());
        dna.dynamic_attributes.insert("category".to_string(), manifest.category.clone());

        // 🔍 BINARY PROBE: Extracting truth directly from GGUF Hardware (Framework-Free)
        if weight_path.exists() {
            info!("🧬 [DNA] Probing weight binary: {:?}", weight_path);
            if let Ok((metadata, _tensor_infos, _tensor_count)) = cluaiz_shared::utils::gguf_prober::GGUFProber::probe(weight_path) {
                // If the engine has sync_with_metadata, call it. If not, we map values manually.
                if let Some(ctx) = metadata.get("llama.context_length").or(metadata.get("qwen2.context_length")) {
                    dna.max_context_length = ctx.parse().ok();
                }
                
                // Store tokenizer configs in dynamic_attributes so they can be written to config.json
                if let Some(chat_tmpl) = metadata.get("tokenizer.chat_template") {
                    dna.chat_template = Some(chat_tmpl.clone());
                }
                if let Some(eos) = metadata.get("tokenizer.ggml.eos_token_id") {
                    dna.eos_token = Some(eos.clone());
                }
                
                info!("🧬 [DNA] Truth-Grounding complete via Binary Header Prober.");
            } else {
                info!("⚠️ [DNA] Failed to probe binary, falling back to Manifest.");
            }
        }

        let dna_path = dest_dir.join("structural_dna.json");
        if let Ok(json) = serde_json::to_string_pretty(&dna) {
            std::fs::write(&dna_path, json).map_err(|e| e.to_string())?;
        }
        

        Ok(())
    }

    async fn download_single_file(
        client: &reqwest::Client,
        url: &str,
        dest_path: &std::path::Path,
        _tx: mpsc::Sender<DownloadEvent>,
        abort: std::sync::Arc<std::sync::atomic::AtomicBool>
    ) -> Result<(), String> {
        let response = client.get(url).send().await.map_err(|e| e.to_string())?;

        if !response.status().is_success() {
            return Err(format!("Download failed for {}: HTTP {}", url, response.status()));
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut downloaded: u64 = 0;
        let mut file = tokio::fs::File::create(dest_path).await.map_err(|e| e.to_string())?;

        // 🚀 NATIVE PROGRESS: The Hugging Face style bar
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"));

        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            if abort.load(std::sync::atomic::Ordering::SeqCst) {
                drop(file);
                let _ = std::fs::remove_file(dest_path);
                return Err("ABORTED".to_string());
            }

            let chunk = item.map_err(|e| e.to_string())?;
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }
        
        pb.finish_with_message("Download Complete");
        Ok(())
    }

    /// 🪄 AUTO-HEAL: Recursively hunts for any missing asset (config, tokenizer, etc.) on Hugging Face.
    pub async fn fetch_asset_auto_heal(repo_id: &str, dest_dir: &std::path::Path, asset_name: &str) -> Result<(), String> {
        let asset_path = dest_dir.join(asset_name);
        if asset_path.exists() { return Ok(()); }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;
        let _model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        
        // 🚀 SMART FALLBACK LIST: Try repo and stripped format dynamically without hardcoded mirror prefix creators
        let mut repo_ids_to_try = vec![repo_id.to_string()];
        let stripped = repo_id.replace("-GGUF", "").replace("-gguf", "");
        if stripped != repo_id {
            repo_ids_to_try.push(stripped);
        }

        for id in repo_ids_to_try {
            let url = format!("https://huggingface.co/{}/resolve/main/{}", id, asset_name);
            let response = match client.get(&url).send().await {
                Ok(res) => res,
                Err(_) => continue, // Skip if request timed out or failed
            };

            // ✅ If we get success, we recover. 
            if response.status().is_success() {
                let mut file = tokio::fs::File::create(&asset_path).await.map_err(|e: std::io::Error| e.to_string())?;
                let mut stream = response.bytes_stream();
                while let Some(item) = stream.next().await {
                    let chunk = item.map_err(|e: reqwest::Error| e.to_string())?;
                    file.write_all(&chunk).await.map_err(|e: std::io::Error| e.to_string())?;
                }
                println!("🪄 [AUTO-HEAL] Recovered '{}' from public repository: {}", asset_name, id);
                return Ok(());
            } else if response.status() == reqwest::StatusCode::UNAUTHORIZED || response.status() == reqwest::StatusCode::FORBIDDEN {
                println!("🛡️ [AUTO-HEAL] Gated repo detected ({}). Skipping...", id);
                continue;
            }
        }
        
        println!("⚠️ [AUTO-HEAL] Optional asset '{}' not found. Continuing safely.", asset_name);
        Ok(())
    }

    pub fn download_gguf(
        category: &str,
        repo_id: &str,
        download_url: &str,
        filename: &str,
        assets: Vec<crate::models::registry::ModelAsset>,
        manifest: Option<ModelManifest>,
        tx: mpsc::Sender<DownloadEvent>,
        abort: Arc<AtomicBool>
    ) -> Result<PathBuf, String> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async { Self::download_gguf_async(category, repo_id, download_url, filename, assets, manifest, tx, abort).await })
    }

    pub fn purge_model(category: &str, repo_id: &str) -> Result<(), String> {
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let path = Self::get_models_dir().join(category).join(model_name);
        if !path.exists() { return Err("Model directory not found".to_string()); }
        for attempt in 1..=3 {
            match std::fs::remove_dir_all(&path) {
                Ok(_) => return Ok(()),
                Err(_e) => { std::thread::sleep(std::time::Duration::from_millis(500 * attempt as u64)); }
            }
        }
        Err("Purge failed after 3 attempts.".to_string())
    }

    pub fn cleanup_partial_download(category: &str, repo_id: &str) -> Result<(), String> {
        let model_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let blobs_path = Self::get_models_dir().join(category).join(model_name).join("blobs");
        if let Ok(entries) = std::fs::read_dir(&blobs_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str());
                if ext == Some("part") || ext == Some("lock") { let _ = std::fs::remove_file(path); }
            }
        }
        Ok(())
    }
}
