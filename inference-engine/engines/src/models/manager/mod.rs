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


        let needs_repair = !weight_file.exists();
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

        // 10. Universal Multi-Part Downloader (GGUF Splits)
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
                                let is_related = if manifest.huggingface_filename.ends_with(".gguf") {
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

        // Dynamically probe model keys and register into model_registry.json
        let format_type = if manifest.huggingface_filename.ends_with(".gguf") { "gguf" } else { "onnx" };
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
