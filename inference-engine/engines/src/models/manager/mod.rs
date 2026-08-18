//! ═══════════════════════════════════════════════════════════════════════
//!   Manager: Sovereign Model Manager & Lifecycle Orchestrator (SSOT)
//! ═══════════════════════════════════════════════════════════════════════

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use colored::Colorize;
use tracing::info;

use crate::models::fetcher::{AssetBundler, AutoHeal, DownloadEvent, FileDownloader, HuggingFaceHub, RegistryClient};
use crate::models::prober::ModelProber;
use crate::models::registry::{CoreRoster, HardwareAuditor, HealthStatus, InstalledStateRegistry, ModelVault};
use crate::models::taxonomy::{SttFamily, SttTaxonomy, TtsFamily, TtsTaxonomy};
use crate::models::types::manifest::{ModelManifest, ModelRegistryEntry, RegistryModelFile};

pub mod auditor {
    pub use crate::models::registry::auditor::*;
}
pub mod client {
    pub use crate::models::fetcher::client::*;
}
pub mod hf_hub {
    pub use crate::models::fetcher::hf_hub::*;
}

pub struct ModelManager {
    client: RegistryClient,
    auditor: HardwareAuditor,
    base_models_dir: PathBuf,
}

impl ModelManager {
    pub fn new(registry_url: String, base_models_dir: PathBuf) -> Self {
        Self {
            client: RegistryClient::new(registry_url),
            auditor: HardwareAuditor,
            base_models_dir,
        }
    }

    pub fn audit_model_health(&self, ram_required_gb: f32, requires_gpu: bool) -> HealthStatus {
        self.auditor.audit_performance(ram_required_gb, requires_gpu)
    }

    /// Pulls a model by ID from local or remote registry
    pub async fn pull_model(&self, model_id: &str) -> Result<(), String> {
        let roster = CoreRoster::load_roster();
        let mut manifest = roster.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());

        if manifest.is_none() {
            let remote_models = CoreRoster::fetch_external_registry(None).await?;
            manifest = remote_models.into_iter().find(|m| m.id.to_lowercase() == model_id.to_lowercase());
        }

        let manifest = manifest.ok_or_else(|| format!("Model ID '{}' not found in any registry.", model_id))?;
        let status = self.audit_model_health(manifest.ram_required_gb as f32, manifest.requires_gpu);
        if status == HealthStatus::Disabled {
            return Err("Audit Failed: Insufficient hardware memory for this model.".to_string());
        }

        self.pull_model_bundle_with_manifest(&manifest, &[manifest.huggingface_filename.clone()]).await
    }

    pub async fn pull_model_with_manifest(&self, manifest: &ModelManifest) -> Result<(), String> {
        self.pull_model_bundle_with_manifest(manifest, &[manifest.huggingface_filename.clone()]).await
    }

    pub async fn pull_model_bundle_with_manifest(&self, manifest: &ModelManifest, all_files: &[String]) -> Result<(), String> {
        let safe_id = manifest.id.replace(':', "-");
        let category_dir = ModelVault::resolve_category_dir(&manifest.category);
        let model_path = category_dir.join(&safe_id);

        tokio::fs::create_dir_all(&model_path)
            .await
            .map_err(|e| format!("Failed to create model directory: {}", e))?;

        if manifest.download_url.contains("huggingface.co") {
            let repo = manifest.download_url.split("/resolve").next().unwrap_or("");
            if !repo.is_empty() {
                println!("  {} [Cluaiz Downloader] Initiating download for {} files...", "📦".cyan(), all_files.len());
                let client = reqwest::Client::new();
                let (tx, _rx) = tokio::sync::mpsc::channel(100);
                let abort = Arc::new(AtomicBool::new(false));

                for (idx, rel_path) in all_files.iter().enumerate() {
                    let rel_lower = rel_path.to_lowercase();
                    let is_voice = rel_lower.contains("voices/") || rel_lower.contains("voice_styles/");
                    let local_file_name = if is_voice {
                        rel_path.clone()
                    } else {
                        Path::new(rel_path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(rel_path)
                            .to_string()
                    };

                    let dest_file = model_path.join(&local_file_name);
                    if let Some(parent) = dest_file.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if !dest_file.exists() {
                        let download_url = format!("{}/resolve/main/{}", repo, rel_path);
                        println!("   ├─ [{}/{}] Fetching {}...", idx + 1, all_files.len(), rel_path.yellow());
                        if let Err(e) = FileDownloader::download_single_file(&client, &download_url, &dest_file, tx.clone(), abort.clone()).await {
                            let _ = tokio::fs::remove_dir_all(&model_path).await;
                            return Err(format!("{}: {}", rel_path, e));
                        } else {
                            println!("   ✅ Weights acquired: {}", local_file_name.green());
                        }
                    } else {
                        println!("   ├─ [{}/{}] Verified {}", idx + 1, all_files.len(), local_file_name.green());
                    }
                }
            }
        }

        // Verify primary weight file exists before auto-healing and registering
        let primary_file = model_path.join(&manifest.huggingface_filename);
        if !primary_file.exists() {
            let _ = tokio::fs::remove_dir_all(&model_path).await;
            return Err(format!("Model weights '{}' missing. Registration aborted.", manifest.huggingface_filename));
        }

        // Auto-heal missing tokenizer or config if needed
        let _ = AutoHeal::auto_heal_missing_asset(&model_path, "tokenizer.json", &["Qwen/Qwen2.5-Coder-7B-Instruct", "meta-llama/Llama-3.2-3B-Instruct"]).await;

        // Save manifest
        if let Ok(manifest_json) = serde_json::to_string_pretty(manifest) {
            let _ = tokio::fs::write(model_path.join("model_manifest.json"), manifest_json).await;
        }

        // Deep Probe and Register into live model_registry.json
        let primary_file = model_path.join(&manifest.huggingface_filename);
        let weight_path = if primary_file.exists() {
            primary_file
        } else {
            model_path.clone()
        };

        let (slot_type, caps, metadata, requires_gpu) = ModelProber::discover(&weight_path, &model_path, &manifest.category);

        let mut files_list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&model_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    let size = p.metadata().map(|m| m.len()).unwrap_or(0);
                    let is_primary = name == manifest.huggingface_filename;
                    files_list.push(RegistryModelFile {
                        name: name.to_string(),
                        size_bytes: size,
                        is_primary,
                    });
                }
            }
        }

        let entry = ModelRegistryEntry {
            id: safe_id,
            category: slot_type.as_str().to_string(),
            format_type: manifest.architecture_type.clone(),
            huggingface_repo: manifest.huggingface_repo.clone(),
            local_dir: model_path.to_string_lossy().to_string(),
            files: files_list,
            extra_files: serde_json::Value::Null,
            supported_tasks: caps.explicit_tasks,
            requires_gpu,
            metadata,
        };

        if let Err(e) = InstalledStateRegistry::register_model(entry) {
            info!("⚠️ [ModelManager] Could not update model_registry.json: {}", e);
        }

        Ok(())
    }
}
