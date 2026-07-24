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

        self.pull_model_bundle_with_manifest(&manifest, &vec![manifest.huggingface_filename.clone()]).await
    }

    /// Installation & Repair: Pull a specific model using an already resolved manifest
    pub async fn pull_model_with_manifest(&self, manifest: &crate::models::registry::ModelManifest) -> Result<(), String> {
        self.pull_model_bundle_with_manifest(manifest, &vec![manifest.huggingface_filename.clone()]).await
    }

    /// Installation & Repair: Pull a specific model bundle using a resolved manifest and variant file list
    pub async fn pull_model_bundle_with_manifest(&self, manifest: &crate::models::registry::ModelManifest, all_files: &[String]) -> Result<(), String> {
        let safe_id = manifest.id.replace(':', "-");
        let mut model_path = self.base_models_dir.clone();
        model_path.push(&manifest.category);
        model_path.push(&safe_id);

        tokio::fs::create_dir_all(&model_path).await
            .map_err(|e| format!("Failed to create model directory: {}", e))?;

        let installer = ModelInstaller::new(model_path.clone());

        if manifest.download_url.contains("huggingface.co") {
            let repo = manifest.download_url.split("/resolve").next().unwrap_or("");
            if !repo.is_empty() {
                println!("  {} [Cluaiz Downloader] Initiating atomic download for {} files...", "📦".cyan(), all_files.len());
                for (idx, rel_path) in all_files.iter().enumerate() {
                    // Strip directory prefixes for flat local storage
                    // e.g. "UD-IQ1_S/model-00001.gguf" → "model-00001.gguf" (flat in vault)
                    let flat_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
                    let dest_file = model_path.join(flat_name);
                    if !dest_file.exists() {
                        let download_url = format!("{}/resolve/main/{}", repo, rel_path);
                        println!("   ├─ [{}/{}] Fetching {}...", idx + 1, all_files.len(), flat_name.yellow());
                        let _ = installer.download_weights(&download_url, flat_name).await;
                    } else {
                        println!("   ├─ [{}/{}] Verified {}", idx + 1, all_files.len(), flat_name.green());
                    }
                }
            }
        } else {
            let weight_file = model_path.join(&manifest.huggingface_filename);
            if !weight_file.exists() {
                installer.download_weights(&manifest.download_url, &manifest.huggingface_filename).await?;
            }
        }

        // Dynamically probe model keys and register into model_registry.json
        let format_type = if manifest.huggingface_filename.ends_with(".gguf") { "gguf" } else { "onnx" };
        let weight_file = model_path.join(&manifest.huggingface_filename);

        let mut architecture = manifest.architecture.clone();
        let mut context_window = manifest.context_window.clone();

        // Perform GGUF metadata probing if applicable
        if format_type == "gguf" {
            if let Ok(metadata) = cluaiz_shared::utils::gguf_prober::GGUFProber::probe(&weight_file) {
                if let Some(arch) = metadata.0.get("general.architecture") {
                    architecture = arch.clone();
                }
                if let Some(ctx) = metadata.0.get("general.context_length") {
                    context_window = ctx.clone();
                }
            }
        }

        // Dynamically resolve SlotType configuration properties via decoupled CapabilityResolver
        let (slot_type, detected_caps, mut metadata, requires_gpu) = cluaiz_shared::utils::model_discovery::CapabilityResolver::discover(
            &weight_file,
            &model_path,
            &manifest.category,
        );

        // If manifest explicitly defines human parameter label (e.g. "Effective 2B", "4B"), prefer manifest parameter definition!
        if !manifest.parameters.trim().is_empty() && manifest.parameters != "Unknown" {
            metadata.parameters = manifest.parameters.clone();
        }

        let category = slot_type.as_str().to_string();
        let supported_tasks = slot_type.supported_tasks(&detected_caps);

        let mut files = vec![cluaiz_shared::utils::RegistryModelFile {
            name: manifest.huggingface_filename.clone(),
            size_bytes: std::fs::metadata(&weight_file).map(|m| m.len()).unwrap_or(0),
            is_primary: true,
        }];

        let mut extra_files = Vec::new();

        // Register any other files downloaded inside directory (e.g. splits, jsons, yamls)
        if let Ok(mut entries) = std::fs::read_dir(&model_path) {
            while let Some(Ok(entry)) = entries.next() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != manifest.huggingface_filename {
                    if name.ends_with(".gguf") || name.ends_with(".onnx") {
                        files.push(cluaiz_shared::utils::RegistryModelFile {
                            name,
                            size_bytes: entry.metadata().map(|m| m.len()).unwrap_or(0),
                            is_primary: false,
                        });
                    } else if name.ends_with(".json") || name.ends_with(".yaml") || name.ends_with(".md") || name.ends_with(".txt") {
                        extra_files.push(name);
                    }
                }
            }
        }

        let registry_entry = cluaiz_shared::utils::ModelRegistryEntry {
            id: safe_id,
            category,
            format_type: format_type.to_string(),
            huggingface_repo: manifest.huggingface_repo.clone(),
            local_dir: model_path.to_string_lossy().to_string(),
            files,
            extra_files,
            supported_tasks,
            requires_gpu,
            metadata,
        };

        if let Err(e) = cluaiz_shared::utils::ModelRegistry::register_model(registry_entry) {
            println!("  {} Failed to register model inside configuration database: {}", "⚠️".yellow(), e);
        } else {
            println!("  {} Model successfully registered inside configuration database.", "📝".green());
        }

        println!("  {} Model '{}' synchronized and ready.\n", "✅".green(), manifest.id);
        Ok(())
    }

    pub fn audit_model_health(&self, ram_required: f32, requires_gpu: bool) -> HealthStatus {
        self.auditor.audit_performance(ram_required, requires_gpu)
    }
}
