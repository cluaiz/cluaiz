//! ═══════════════════════════════════════════════════════════════════════
//!   Registry: Autonomous Local Discovery & DNA Indexing
//! ═══════════════════════════════════════════════════════════════════════

use std::fs;
use std::path::Path;
use tracing::info;
use crate::models::types::manifest::ModelManifest;
use crate::models::taxonomy::classifier::UniversalModelClassifier;

pub struct AutonomousDiscovery;

impl AutonomousDiscovery {
    /// Deep-scans the models directory for local units and DNA skeletons
    pub fn index_Cluaiz_models(base_path: &Path) -> Vec<ModelManifest> {
        let mut models = Vec::new();
        if !base_path.exists() {
            return models;
        }

        Self::scan_recursive(base_path, &mut models);
        models
    }

    fn scan_recursive(dir: &Path, models: &mut Vec<ModelManifest>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let manifest_path = path.join("model_manifest.json");
                    let dna_path = path.join("structural_dna.json");

                    if manifest_path.exists() {
                        if let Ok(content) = fs::read_to_string(&manifest_path) {
                            if let Ok(mut manifest) = serde_json::from_str::<ModelManifest>(&content) {
                                manifest.local_path = Some(path.to_string_lossy().to_string());

                                // 🧬 DNA HEALING: Trigger regeneration if DNA is missing or has nulls
                                let mut needs_healing = !dna_path.exists();
                                if !needs_healing {
                                    if let Ok(dna_str) = fs::read_to_string(&dna_path) {
                                        if dna_str.contains(": null") || dna_str.contains(":null") {
                                            needs_healing = true;
                                            info!("🧬 [Healing] Null fields detected for '{}'. Regenerating...", manifest.id);
                                        }
                                    }
                                }

                                if needs_healing {
                                    let _ = Self::repair_dna_from_local(&path, &manifest);
                                }

                                if dna_path.exists() {
                                    manifest.dna_path = Some(dna_path.to_string_lossy().to_string());
                                }
                                models.push(manifest);
                                continue;
                            }
                        }
                    }

                    // Check if this directory contains model files directly
                    if let Some(manifest) = Self::synthesize_manifest_for_dir(&path) {
                        models.push(manifest);
                    } else {
                        Self::scan_recursive(&path, models);
                    }
                }
            }
        }
    }

    /// Synthesizes a fresh ModelManifest and DNA skeleton for an unindexed directory
    fn synthesize_manifest_for_dir(dir: &Path) -> Option<ModelManifest> {
        let mut file_names = Vec::new();
        let mut primary_weight_file = None;
        let mut total_size_bytes = 0u64;

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    file_names.push(name.to_string());
                    if let Ok(meta) = p.metadata() {
                        total_size_bytes += meta.len();
                    }
                    let lower = name.to_lowercase();
                    if (lower.ends_with(".gguf") || lower.ends_with(".onnx")) && primary_weight_file.is_none() {
                        primary_weight_file = Some(name.to_string());
                    }
                }
            }
        }

        let primary_name = primary_weight_file?;
        let model_id = dir.file_name().and_then(|s| s.to_str())?.to_string();
        let primary_path = dir.join(&primary_name);

        // Determine category folder hint if any
        let category_hint = dir.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("");
        
        // Leverage autonomous ModelProber for deep binary probing
        let (slot_type, caps, meta, requires_gpu) = crate::models::prober::ModelProber::discover(
            &primary_path,
            dir,
            category_hint,
        );

        let size_gb = total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let category = slot_type.as_str().to_string();

        let manifest = ModelManifest {
            id: model_id.clone(),
            name: model_id.clone(),
            architecture: meta.architecture.clone(),
            architecture_type: "dense".to_string(),
            parameters: meta.parameters.clone(),
            training_tokens: "Unknown".to_string(),
            bit_depth: meta.bit_depth.and_then(|b| b.parse::<f64>().ok()).unwrap_or(4.0),
            ram_required_gb: size_gb * 1.2,
            download_size_gb: size_gb,
            huggingface_repo: model_id.clone(),
            huggingface_filename: primary_name,
            download_url: String::new(),
            description: format!("Discovered local {} model '{}'", category, model_id),
            is_cloud_api: false,
            requires_gpu,
            is_free_tier: true,
            input_modality: category.clone(),
            context_window: meta.context_window.clone(),
            family: model_id,
            category,
            assets: Vec::new(),
            local_path: Some(dir.to_string_lossy().to_string()),
            dna_path: None,
            has_vision: caps.has_vision || caps.is_vision_chat,
            has_audio: caps.is_tts || caps.is_asr || caps.is_audio_to_audio,
            expert_count: None,
            experts_per_token: None,
        };

        // Create DNA skeleton and save manifest
        let _ = Self::repair_dna_from_local(dir, &manifest);
        if let Ok(manifest_json) = serde_json::to_string_pretty(&manifest) {
            let _ = fs::write(dir.join("model_manifest.json"), manifest_json);
        }

        Some(manifest)
    }

    /// 🩹 DNA Creation / Healing: Generates and seals structural_dna.json
    fn repair_dna_from_local(dir: &Path, manifest: &ModelManifest) -> Result<(), String> {
        let dna = cluaiz_shared::StructuralDNA::create_skeleton(
            manifest.id.clone(),
            manifest.has_vision,
            manifest.expert_count,
            manifest.bit_depth,
            &manifest.context_window,
        );

        let dna_path = dir.join("structural_dna.json");
        if let Ok(dna_json) = serde_json::to_string_pretty(&dna) {
            let _ = fs::write(&dna_path, dna_json);
        }

        Ok(())
    }
}
