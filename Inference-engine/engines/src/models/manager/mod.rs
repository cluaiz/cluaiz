use std::path::PathBuf;
use colored::Colorize;
use crate::models::manager::client::RegistryClient;
use crate::models::manager::installer::ModelInstaller;
use crate::models::manager::auditor::{HardwareAuditor, HealthStatus};

pub mod client;
pub mod installer;
pub mod auditor;
pub mod hf_hub;

/// The Cluaiz Model Manager
/// Responsible for model discovery, health auditing, and atomic installation/repair.
pub struct ModelManager {
    client: RegistryClient,
    installer: ModelInstaller,
    auditor: HardwareAuditor,
    base_models_dir: PathBuf,
}

impl ModelManager {
    pub fn new(registry_url: String, base_models_dir: PathBuf) -> Self {
        Self {
            client: RegistryClient::new(registry_url),
            installer: ModelInstaller::new(base_models_dir.clone()),
            auditor: HardwareAuditor,
            base_models_dir,
        }
    }

    /// Installation & Repair: Pull a specific model by its Unified ID (e.g., bonsai:8b)
    pub async fn pull_model(&self, model_id: &str) -> Result<(), String> {
        // 1. Resolve Metadata
        let roster = crate::models::registry::CoreRoster::load_roster();
        let mut manifest = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());

        if manifest.is_none() {
            let remote_models = crate::models::registry::CoreRoster::fetch_external_registry(None).await?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }

        let manifest = manifest.ok_or_else(|| format!("ID '{}' not found in any registry.", model_id))?;

        // 2. Hardware Audit
        let status = self.audit_model_health(manifest.ram_required_gb as f32, manifest.requires_gpu);
        if status == HealthStatus::Disabled {
            return Err("Cluaiz Audit Failed: Insufficient hardware resources for this model.".to_string());
        }

        self.pull_model_with_manifest(&manifest).await
    }

    /// Installation & Repair: Pull a specific model using an already resolved manifest
    pub async fn pull_model_with_manifest(&self, manifest: &crate::models::registry::ModelManifest) -> Result<(), String> {


        // 3. Construct Path
        let safe_id = manifest.id.replace(':', "-");
        let mut model_path = self.base_models_dir.clone();
        model_path.push(&manifest.category);
        model_path.push(&safe_id);

        tokio::fs::create_dir_all(&model_path).await
            .map_err(|e| format!("Failed to create model directory: {}", e))?;

        // 4. Initialize Installer
        let installer = ModelInstaller::new(model_path.clone());

        // 5. Check Weights & Assets (The Surgical Repair)
        let weight_file = model_path.join(&manifest.huggingface_filename);
        let dna_file = model_path.join("structural_dna.json");

        let needs_repair = !weight_file.exists() || !dna_file.exists();
        // Check if any asset is missing (Removed external JSON checks, only check weights and DNA)
        if !needs_repair {
            println!("  {} Model '{}' is healthy and ready.", "✅".green(), manifest.id);
            return Ok(());
        }

        // 6. Pull Missing Weights
        if !weight_file.exists() {
            installer.download_weights(&manifest.download_url, &manifest.huggingface_filename).await?;
        } else {
            println!("  {} Weights verified.", "✅".green());
        }

        let tokenizer_file = model_path.join("tokenizer.json");
        if !tokenizer_file.exists() && manifest.download_url.contains("huggingface.co") {
            // Attempt to fetch tokenizer.json, ignore failure if not present.
            let repo = manifest.download_url.split("/resolve").next().unwrap_or("");
            if !repo.is_empty() {
                let tokenizer_url = format!("{}/resolve/main/tokenizer.json", repo);
                println!("  {} Synchronizing native tokenizer...", "🔍".cyan());
                let _ = installer.download_weights(&tokenizer_url, "tokenizer.json").await;
            }
        }

        // 10. Universal Multi-Part Downloader (ONNX Data & GGUF Splits)
        if manifest.download_url.contains("huggingface.co") {
            let repo = manifest.download_url.split("/resolve").next().unwrap_or("");
            if !repo.is_empty() {
                let api_url = format!("{}/tree/main?recursive=true", repo.replace("huggingface.co/", "huggingface.co/api/models/"));
                let client = reqwest::Client::new();
                if let Ok(res) = client.get(&api_url).send().await {
                    if let Ok(items) = res.json::<Vec<serde_json::Value>>().await {
                        let base_name = std::path::Path::new(&manifest.huggingface_filename)
                            .file_name().and_then(|n| n.to_str()).unwrap_or(&manifest.huggingface_filename);
                        
                        let base_prefix = if base_name.ends_with(".gguf") && base_name.contains("-of-") {
                            base_name.split("-0").next().unwrap_or(base_name)
                        } else {
                            base_name
                        };

                        for item in items {
                            if let Some(path) = item.get("path").and_then(|p| p.as_str()) {
                                let is_related = if manifest.huggingface_filename.ends_with(".onnx") {
                                    path.starts_with(&format!("{}_data", base_name))
                                } else if manifest.huggingface_filename.ends_with(".gguf") {
                                    path.starts_with(base_prefix) && path.ends_with(".gguf") && path != base_name
                                } else {
                                    false
                                };

                                if is_related {
                                    let part_file = model_path.join(path);
                                    if !part_file.exists() {
                                        let part_url = format!("{}/resolve/main/{}", repo, path);
                                        println!("  {} Resolving Multi-Part Split: Fetching {}...", "🧠".cyan(), path);
                                        let _ = installer.download_weights(&part_url, path).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 8. Save/Refresh local manifest
        let local_manifest_path = model_path.join("model_manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("JSON Serialize Error: {}", e))?;
        tokio::fs::write(local_manifest_path, manifest_json).await
            .map_err(|e| format!("Failed to save local manifest: {}", e))?;

        // 9. 🧬 Neural DNA Handshake (Always ensure DNA is fresh)
        let _ = crate::models::fetch::ModelDownloader::generate_cluaiz_dna(&manifest, &model_path, &weight_file);

        println!("  {} Model '{}' synchronized and ready.\n", "✅".green(), manifest.id);
        Ok(())
    }

    pub fn audit_model_health(&self, ram_required: f32, requires_gpu: bool) -> HealthStatus {
        self.auditor.audit_performance(ram_required, requires_gpu)
    }
}
