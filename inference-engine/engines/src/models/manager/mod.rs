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
                    let rel_lower = rel_path.to_lowercase();
                    let is_voice = rel_lower.contains("voices/") || rel_lower.contains("voice_styles/") || rel_lower.contains("espeak-ng-data/");
                    let local_file_name = if is_voice {
                        rel_path.clone()
                    } else {
                        std::path::Path::new(rel_path).file_name().and_then(|s| s.to_str()).unwrap_or(rel_path).to_string()
                    };

                    let dest_file = model_path.join(&local_file_name);
                    if let Some(parent) = dest_file.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if !dest_file.exists() {
                        let download_url = format!("{}/resolve/main/{}", repo, rel_path);
                        println!("   ├─ [{}/{}] Fetching {}...", idx + 1, all_files.len(), rel_path.yellow());
                        if let Err(e) = installer.download_weights(&download_url, &local_file_name).await {
                            println!("   ❌ Failed to download {}: {}", rel_path, e);
                        } else {
                            println!("   ✅ Weights acquired: {}", local_file_name.green());
                        }
                    } else {
                        println!("   ├─ [{}/{}] Verified {}", idx + 1, all_files.len(), local_file_name.green());
                    }
                }
            }
        } else {
            let weight_file = model_path.join(&manifest.huggingface_filename);
            if !weight_file.exists() {
                installer.download_weights(&manifest.download_url, &manifest.huggingface_filename).await?;
            }
        }
        
        // 🌐 AUTO-FETCH HF API METADATA FOR 3-WAY DISCOVERY VOTING
        if manifest.download_url.contains("huggingface.co") {
            let repo = manifest.download_url.split("/resolve").next().unwrap_or("").replace("https://huggingface.co/", "");
            if !repo.is_empty() {
                let api_url = format!("https://huggingface.co/api/models/{}", repo);
                let client = reqwest::Client::new();
                if let Ok(res) = client.get(&api_url).send().await {
                    if res.status().is_success() {
                        if let Ok(bytes) = res.bytes().await {
                            let hf_meta_path = model_path.join("hf_metadata.json");
                            if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                                if let Ok(pretty) = serde_json::to_string_pretty(&json_val) {
                                    let _ = tokio::fs::write(&hf_meta_path, pretty).await;
                                }
                            }
                        }
                    }
                }
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

        let mut weight_files = Vec::new();
        let mut extra_files_list = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&model_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() {
                    let mut subfolder_files = Vec::new();
                    if let Ok(sub_entries) = std::fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            let sub_name = sub_entry.file_name().to_string_lossy().to_string();
                            subfolder_files.push(serde_json::Value::String(sub_name));
                        }
                    }
                    let mut sub_map = serde_json::Map::new();
                    sub_map.insert(name, serde_json::Value::Array(subfolder_files));
                    extra_files_list.push(serde_json::Value::Object(sub_map));
                } else {
                    let lower = name.to_lowercase();
                    if lower.ends_with(".gguf") || lower.ends_with(".onnx") {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        weight_files.push((name, size));
                    } else if lower.ends_with(".json") || lower.ends_with(".yaml") || lower.ends_with(".md") || lower.ends_with(".txt") || lower.ends_with(".bin") {
                        extra_files_list.push(serde_json::Value::String(name));
                    }
                }
            }
        }
        let extra_files = serde_json::Value::Array(extra_files_list);

        // Evaluate all weight files using AssetResolver priority scoring to find the true Primary Graph
        let mut best_score = usize::MAX;
        let mut primary_name = manifest.huggingface_filename.clone();

        for (name, _) in &weight_files {
            let score = crate::models::fetch::AssetResolver::score_model_file_priority(&category, name);
            if score < best_score {
                best_score = score;
                primary_name = name.clone();
            }
        }

        let mut files = Vec::new();
        for (name, size_bytes) in weight_files {
            let is_primary = name == primary_name;
            files.push(cluaiz_shared::utils::RegistryModelFile {
                name,
                size_bytes,
                is_primary,
            });
        }

        let mut tts_family = detected_caps.tts_family.clone();
        if tts_family.is_none() && category == "audio" {
            tts_family = Some(crate::models::fetch::tts_resolver::TtsAssetResolver::detect_tts_family(&manifest.id, &extra_files.to_string()).to_string());
        }

        // Run Fail-Fast Package Contract Validation Gate for Audio Models
        if category == "audio" {
            if let Some(ref fam) = tts_family {
                if let Err(contract_err) = crate::models::fetch::tts_resolver::TtsAssetResolver::validate_family_package_contract(fam, &model_path) {
                    println!("  {} [TTS Manifest Gate] Package contract warning: {}", "⚠️".yellow(), contract_err);
                } else {
                    println!("  {} [TTS Manifest Gate] Package contract verified for family '{}'.", "✅".green(), fam);
                }
            }
        }

        metadata.tts_family = tts_family;
        metadata.backend_type = Some(if format_type == "gguf" { "ggml".to_string() } else { "onnx".to_string() });

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
