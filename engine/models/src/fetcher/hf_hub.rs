//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Industrial HuggingFace Hub Multi-Shard & Variant Resolver
//! ═══════════════════════════════════════════════════════════════════════

use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use crate::models::types::manifest::ModelManifest;
use crate::models::taxonomy::classifier::UniversalModelClassifier;
use crate::models::taxonomy::quantization::UniversalQuantization;
use crate::models::taxonomy::tts_families::TtsTaxonomy;
use crate::models::fetcher::asset_bundler::AssetBundler;

#[derive(Debug, Deserialize, Clone)]
pub struct HfTreeItem {
    pub path: String,
    pub size: Option<u64>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HfVariant {
    pub variant_id: String,
    pub format_type: String,
    pub quant_tag: String,
    pub primary_file: String,
    pub all_files: Vec<String>,
    pub filename: String,
    pub size_gb: f64,
}

pub struct HuggingFaceHub;

impl HuggingFaceHub {
    /// Resolves Hugging Face token from environment variables or standard cache files
    pub fn resolve_hf_token() -> Option<String> {
        if let Ok(token) = std::env::var("HF_TOKEN") {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        if let Ok(token) = std::env::var("HUGGING_FACE_HUB_TOKEN") {
            let trimmed = token.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            let cache_token_path = std::path::PathBuf::from(&home).join(".cache").join("huggingface").join("token");
            if cache_token_path.exists() {
                if let Ok(content) = std::fs::read_to_string(cache_token_path) {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
            let legacy_token_path = std::path::PathBuf::from(&home).join(".huggingface").join("token");
            if legacy_token_path.exists() {
                if let Ok(content) = std::fs::read_to_string(legacy_token_path) {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }

        None
    }

    /// Fetch raw file tree items from HuggingFace repository with pagination support
    pub async fn list_raw_tree(repo_id: &str) -> Result<Vec<HfTreeItem>, String> {
        let client = Client::new();
        let mut url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=true", repo_id);
        let mut items = Vec::new();
        let token_opt = Self::resolve_hf_token();

        loop {
            let mut req = client.get(&url);
            if let Some(ref token) = token_opt {
                req = req.header("Authorization", format!("Bearer {}", token));
            }
            let response = req.send().await.map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                let status = response.status();
                if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
                    return Err(format!(
                        "🔒 [Gated Model Authentication Required]\nRepository '{}' is gated on Hugging Face.\n1. Accept license terms at: https://huggingface.co/{}\n2. Set your Hugging Face Token:\n   PowerShell:   $env:HF_TOKEN=\"hf_your_token\"\n   Windows CMD:  set HF_TOKEN=hf_your_token\n   Linux / Mac:  export HF_TOKEN=\"hf_your_token\"\n3. Run the command again.",
                        repo_id, repo_id
                    ));
                }
                return Err(format!("Failed to fetch repository '{}' (HTTP {}). Does it exist?", repo_id, status));
            }

            let link_header = response.headers()
                .get(reqwest::header::LINK)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let page_items: Vec<HfTreeItem> = response.json().await.map_err(|e| e.to_string())?;
            items.extend(page_items);

            let mut next_url = None;
            if let Some(ref lh) = link_header {
                for part in lh.split(',') {
                    let part = part.trim();
                    if part.contains("rel=\"next\"") {
                        if let Some(start) = part.find('<') {
                            if let Some(end) = part.find('>') {
                                if start < end {
                                    next_url = Some(part[start + 1..end].to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(next) = next_url {
                url = next;
            } else {
                break;
            }
        }
        Ok(items)
    }

    /// List all supported model variants (GGUF, ONNX) grouped into cohesive bundles with multi-shard support
    pub async fn list_variants(repo_id: &str) -> Result<Vec<HfVariant>, String> {
        let items = Self::list_raw_tree(repo_id).await?;
        
        let metadata_files: Vec<String> = items.iter().filter_map(|item| {
            let path = &item.path;
            let lower = path.to_lowercase();
            if lower.ends_with(".json") 
                || lower.ends_with(".txt") 
                || lower.ends_with(".yaml")
                || lower.ends_with(".yml")
                || lower.ends_with("vocab.json") 
                || lower.ends_with("merges.txt") 
                || lower.ends_with("cluaiz-engine.ready")
                || lower.contains("voices/")
                || lower.contains("voice_styles/")
                || lower.contains("vocoder/")
                || lower.contains("codec/")
                || lower.ends_with("speaker_embeddings.bin")
            {
                Some(path.clone())
            } else {
                None
            }
        }).collect();

        let mut variants = Vec::new();

        // 1. Group GGUF models (Handling single files + multi-part shards + mmproj / MTP helpers)
        let mut shard_groups: HashMap<String, Vec<&HfTreeItem>> = HashMap::new();
        let mut standalone_ggufs: Vec<&HfTreeItem> = Vec::new();
        let mut mmproj_files: Vec<&str> = Vec::new();
        let mut mtp_files: Vec<&str> = Vec::new();

        for item in &items {
            let lower = item.path.to_lowercase();
            if lower.ends_with(".gguf") {
                if AssetBundler::is_mmproj_gguf(&lower) {
                    mmproj_files.push(&item.path);
                } else if AssetBundler::is_mtp_gguf(&lower) {
                    mtp_files.push(&item.path);
                } else if !AssetBundler::is_helper_gguf(&lower) {
                    if let Some(base) = UniversalQuantization::extract_shard_base(&item.path) {
                        shard_groups.entry(base).or_default().push(item);
                    } else {
                        standalone_ggufs.push(item);
                    }
                }
            }
        }

        // Add standalone GGUFs
        for item in standalone_ggufs {
            let size_gb = item.size.unwrap_or(0) as f64 / (1024.0 * 1024.0 * 1024.0);
            let fname = Path::new(&item.path).file_name().and_then(|s| s.to_str()).unwrap_or(&item.path).to_string();
            let quant_tag = UniversalQuantization::extract_quant_tag(&fname);

            let mut files = vec![item.path.clone()];

            if let Some(best_mm) = AssetBundler::select_best_mmproj(&mmproj_files) {
                files.push(best_mm.to_string());
            }

            if let Some(best_mtp) = AssetBundler::select_best_mtp(&mtp_files, &quant_tag) {
                files.push(best_mtp.to_string());
            }

            for mf in &metadata_files {
                if AssetBundler::is_compatible_subfolder_metadata(mf, &item.path) {
                    files.push(mf.clone());
                }
            }
            AssetBundler::filter_duplicate_metadata_files(&mut files);

            variants.push(HfVariant {
                variant_id: fname.clone(),
                format_type: "gguf".to_string(),
                quant_tag,
                primary_file: item.path.clone(),
                all_files: files,
                filename: fname,
                size_gb,
            });
        }

        // Add sharded GGUFs
        for (_base, mut shards) in shard_groups {
            shards.sort_by_key(|s| &s.path);
            if let Some(first_shard) = shards.first() {
                let total_size_bytes: u64 = shards.iter().map(|s| s.size.unwrap_or(0)).sum();
                let size_gb = total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let fname = Path::new(&first_shard.path).file_name().and_then(|s| s.to_str()).unwrap_or(&first_shard.path).to_string();
                let quant_tag = UniversalQuantization::extract_quant_tag(&fname);

                let mut files: Vec<String> = shards.iter().map(|s| s.path.clone()).collect();

                if let Some(best_mm) = AssetBundler::select_best_mmproj(&mmproj_files) {
                    files.push(best_mm.to_string());
                }
                if let Some(best_mtp) = AssetBundler::select_best_mtp(&mtp_files, &quant_tag) {
                    files.push(best_mtp.to_string());
                }

                for mf in &metadata_files {
                    if AssetBundler::is_compatible_subfolder_metadata(mf, &first_shard.path) {
                        files.push(mf.clone());
                    }
                }
                AssetBundler::filter_duplicate_metadata_files(&mut files);

                variants.push(HfVariant {
                    variant_id: fname.clone(),
                    format_type: "gguf".to_string(),
                    quant_tag,
                    primary_file: first_shard.path.clone(),
                    all_files: files,
                    filename: fname,
                    size_gb,
                });
            }
        }

        // 2. Process ONNX models (Bundling .onnx.data companion binary shards via SSOT)
        for item in &items {
            let lower = item.path.to_lowercase();
            if lower.ends_with(".onnx") && !TtsTaxonomy::is_subcomponent_file(&item.path) {
                let mut bundle_files = vec![item.path.clone()];
                let mut total_size = item.size.unwrap_or(0);
                let parent_dir = Path::new(&item.path).parent().and_then(|p| p.to_str()).unwrap_or("");
                let clean_base = UniversalQuantization::strip_onnx_companion_suffix(&item.path);

                // Pair matching companion .onnx.data shards
                for data_item in &items {
                    let d_path = &data_item.path;
                    let d_parent = Path::new(d_path).parent().and_then(|p| p.to_str()).unwrap_or("");

                    if UniversalQuantization::is_onnx_companion_file(d_path) && (d_parent == parent_dir || d_path.starts_with(parent_dir)) {
                        if UniversalQuantization::strip_onnx_companion_suffix(d_path) == clean_base {
                            bundle_files.push(d_path.clone());
                            total_size += data_item.size.unwrap_or(0);
                        }
                    }
                }

                // Add auxiliary metadata
                for mf in &metadata_files {
                    if AssetBundler::is_compatible_subfolder_metadata(mf, &item.path) {
                        bundle_files.push(mf.clone());
                    }
                }
                AssetBundler::filter_duplicate_metadata_files(&mut bundle_files);

                let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                let fname = Path::new(&item.path).file_name().and_then(|s| s.to_str()).unwrap_or(&item.path).to_string();
                let quant_tag = UniversalQuantization::extract_quant_tag(&fname);

                variants.push(HfVariant {
                    variant_id: fname.clone(),
                    format_type: "onnx".to_string(),
                    quant_tag,
                    primary_file: item.path.clone(),
                    all_files: bundle_files,
                    filename: fname,
                    size_gb,
                });
            }
        }

        if variants.is_empty() {
            return Err(format!("No supported model files (.gguf, .onnx) found in repository '{}'.", repo_id));
        }

        Ok(variants)
    }

    /// Builds a canonical ModelManifest for a Hugging Face variant
    pub fn build_manifest(repo_id: &str, variant: &HfVariant, pipeline_tag: Option<&str>) -> ModelManifest {
        let classification = UniversalModelClassifier::classify(
            repo_id,
            pipeline_tag,
            &[],
            &variant.all_files,
            None,
        );

        let safe_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let download_url = format!("https://huggingface.co/{}/resolve/main/{}", repo_id, variant.primary_file);
        let bit_depth = UniversalQuantization::estimate_bit_depth(&variant.quant_tag);
        let parameters = UniversalQuantization::extract_parameters_from_name(&variant.filename, &variant.quant_tag);
        let sovereign_id = AssetBundler::resolve_sovereign_id(repo_id, safe_name, &variant.filename, &variant.quant_tag);

        ModelManifest {
            id: sovereign_id,
            name: safe_name.to_string(),
            architecture: "transformer".to_string(),
            architecture_type: variant.format_type.clone(),
            parameters,
            training_tokens: "Unknown".to_string(),
            bit_depth,
            ram_required_gb: variant.size_gb * 1.2 + 0.5,
            download_size_gb: variant.size_gb,
            huggingface_repo: repo_id.to_string(),
            huggingface_filename: variant.primary_file.clone(),
            download_url,
            description: format!("Model {} ({})", safe_name, variant.quant_tag),
            is_cloud_api: false,
            requires_gpu: false,
            is_free_tier: true,
            input_modality: classification.category.clone(),
            context_window: "8k".to_string(),
            family: safe_name.to_string(),
            category: classification.category,
            assets: Vec::new(),
            local_path: None,
            dna_path: None,
            has_vision: classification.capabilities.has_vision,
            has_audio: classification.capabilities.is_tts || classification.capabilities.is_asr,
            expert_count: None,
            experts_per_token: None,
        }
    }
}
