//! ═══════════════════════════════════════════════════════════════════════
//!   Fetcher: Industrial HuggingFace Hub Multi-Shard & Variant Resolver
//! ═══════════════════════════════════════════════════════════════════════

use crate::models::fetcher::asset_bundler::AssetBundler;
use crate::models::taxonomy::classifier::UniversalModelClassifier;
use crate::models::taxonomy::quantization::UniversalQuantization;
use crate::models::taxonomy::stt_families::SttTaxonomy;
use crate::models::taxonomy::tts_families::TtsTaxonomy;
use crate::models::types::manifest::ModelManifest;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct HfTreeItem {
    pub path: String,
    pub size: Option<u64>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HfVariant {
    pub revision: Option<String>,
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
    /// Resolve an existing primary/component to exactly one full repository bundle.
    /// Used by repair and single-primary install callers; ambiguity never picks the first model.
    pub async fn bundle_for_manifest(
        manifest: &ModelManifest,
    ) -> Result<(ModelManifest, Vec<String>), String> {
        let source = Self::primary_repository_path(manifest);
        let variants = Self::list_variants(&manifest.huggingface_repo).await?;
        let mut matches = variants
            .into_iter()
            .filter(|v| v.all_files.iter().any(|file| file == source));
        let variant = matches
            .next()
            .ok_or("No compatible repair bundle; use the explicit repository and variant")?;
        if matches.next().is_some() {
            return Err("Ambiguous package source; choose an explicit repository variant".into());
        }
        let mut resolved = Self::build_manifest(&manifest.huggingface_repo, &variant, None, &[]);
        resolved.id = manifest.id.clone();
        resolved.category = manifest.category.clone();
        resolved.huggingface_filename = variant
            .primary_file
            .rsplit('/')
            .next()
            .unwrap_or(&variant.primary_file)
            .to_owned();
        Ok((resolved, variant.all_files))
    }

    /// The existing primary download URL carries the chosen snapshot, avoiding
    /// a registry schema migration. Legacy manifests retain their main revision.
    pub fn manifest_revision(manifest: &ModelManifest) -> &str {
        let prefix = format!(
            "https://huggingface.co/{}/resolve/",
            manifest.huggingface_repo
        );
        manifest
            .download_url
            .strip_prefix(&prefix)
            .and_then(|path| path.split('/').next())
            .filter(|revision| {
                *revision == "main"
                    || (revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()))
            })
            .unwrap_or("main")
    }

    pub fn asset_url(manifest: &ModelManifest, asset: &str) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            manifest.huggingface_repo,
            Self::manifest_revision(manifest),
            asset
        )
    }

    pub fn primary_repository_path(manifest: &ModelManifest) -> &str {
        let prefix = format!(
            "https://huggingface.co/{}/resolve/{}/",
            manifest.huggingface_repo,
            Self::manifest_revision(manifest)
        );
        manifest
            .download_url
            .strip_prefix(&prefix)
            .unwrap_or(&manifest.huggingface_filename)
    }

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
            let cache_token_path = std::path::PathBuf::from(&home)
                .join(".cache")
                .join("huggingface")
                .join("token");
            if cache_token_path.exists() {
                if let Ok(content) = std::fs::read_to_string(cache_token_path) {
                    let trimmed = content.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
            let legacy_token_path = std::path::PathBuf::from(&home)
                .join(".huggingface")
                .join("token");
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

    /// What the repository declares it does: its pipeline tag and tags (empty when the Hub cannot
    /// be read, and the model is then classified from its files once downloaded)
    pub async fn declared_tasks(repo_id: &str) -> (Option<String>, Vec<String>) {
        #[derive(serde::Deserialize)]
        struct Info {
            pipeline_tag: Option<String>,
            #[serde(default)]
            tags: Vec<String>,
        }
        let mut req = Client::new().get(format!("https://huggingface.co/api/models/{repo_id}"));
        if let Some(token) = Self::resolve_hf_token() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        match req.send().await.and_then(|r| r.error_for_status()) {
            Ok(response) => match response.json::<Info>().await {
                Ok(info) => (info.pipeline_tag, info.tags),
                Err(_) => (None, Vec::new()),
            },
            Err(_) => (None, Vec::new()),
        }
    }

    /// Fetch raw file tree items from HuggingFace repository with pagination support
    pub async fn list_raw_tree(repo_id: &str) -> Result<Vec<HfTreeItem>, String> {
        Self::list_raw_tree_at(repo_id, "main").await
    }

    pub async fn list_raw_tree_at(
        repo_id: &str,
        revision: &str,
    ) -> Result<Vec<HfTreeItem>, String> {
        let client = Client::new();
        let mut url = format!(
            "https://huggingface.co/api/models/{}/tree/{}?recursive=true",
            repo_id, revision
        );
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
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    return Err(format!(
                        "🔒 [Gated Model Authentication Required]\nRepository '{}' is gated on Hugging Face.\n1. Accept license terms at: https://huggingface.co/{}\n2. Set your Hugging Face Token:\n   PowerShell:   $env:HF_TOKEN=\"hf_your_token\"\n   Windows CMD:  set HF_TOKEN=hf_your_token\n   Linux / Mac:  export HF_TOKEN=\"hf_your_token\"\n3. Run the command again.",
                        repo_id, repo_id
                    ));
                }
                return Err(format!(
                    "Failed to fetch repository '{}' (HTTP {}). Does it exist?",
                    repo_id, status
                ));
            }

            let link_header = response
                .headers()
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

        let metadata_files: Vec<String> = items
            .iter()
            .filter_map(|item| {
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
            })
            .collect();

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
            let fname = Path::new(&item.path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&item.path)
                .to_string();
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
            AssetBundler::filter_duplicate_metadata_files(&mut files, &item.path);

            variants.push(HfVariant {
                revision: None,
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
                let fname = Path::new(&first_shard.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&first_shard.path)
                    .to_string();
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
                AssetBundler::filter_duplicate_metadata_files(&mut files, &first_shard.path);

                variants.push(HfVariant {
                    revision: None,
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

        // 2. Process ONNX models (Bundling .onnx.data companion binary shards & STT sub-graphs via SSOT)
        variants.extend(Self::bundle_onnx_variants(&items, &metadata_files));
        super::package_bundler::complete_variants(repo_id, &items, &mut variants).await?;

        if variants.is_empty() {
            return Err(format!(
                "No supported model files (.gguf, .onnx) found in repository '{}'.",
                repo_id
            ));
        }

        Ok(variants)
    }

    /// Bundles primary `.onnx` models with their matching STT sub-component graphs
    /// (e.g. `decoder_model_merged.onnx`, `uncached_decode.onnx`) and `.onnx_data` shards.
    pub fn bundle_onnx_variants(items: &[HfTreeItem], metadata_files: &[String]) -> Vec<HfVariant> {
        let mut variants = Vec::new();
        let has_stt_encoder_in_dir = |dir: &str| -> bool {
            items.iter().any(|candidate| {
                let c_lower = candidate.path.to_lowercase();
                let c_parent = Path::new(&candidate.path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or("");
                c_parent == dir
                    && c_lower.ends_with(".onnx")
                    && !TtsTaxonomy::is_subcomponent_file(&candidate.path)
                    && !SttTaxonomy::is_subcomponent_file(&candidate.path)
            })
        };

        for item in items {
            let lower = item.path.to_lowercase();
            let parent_dir = Path::new(&item.path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");
            let is_stt_subcomponent =
                SttTaxonomy::is_subcomponent_file(&item.path) && has_stt_encoder_in_dir(parent_dir);
            if lower.ends_with(".onnx")
                && !TtsTaxonomy::is_subcomponent_file(&item.path)
                && !is_stt_subcomponent
            {
                let mut bundle_files = vec![item.path.clone()];
                let mut total_size = item.size.unwrap_or(0);
                let fname = Path::new(&item.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&item.path)
                    .to_string();
                let quant_tag = UniversalQuantization::extract_quant_tag(&fname);

                // Pair companion STT sub-component .onnx graphs (e.g. decoder_model*.onnx, uncached_decode.onnx) in same dir with matching quantization
                for sub_item in items {
                    let s_path = &sub_item.path;
                    let s_lower = s_path.to_lowercase();
                    let s_parent = Path::new(s_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");
                    if s_path != &item.path
                        && s_parent == parent_dir
                        && s_lower.ends_with(".onnx")
                        && SttTaxonomy::is_subcomponent_file(s_path)
                    {
                        let s_fname = Path::new(s_path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(s_path);
                        let s_quant = UniversalQuantization::extract_quant_tag(s_fname);
                        if s_quant == quant_tag && !bundle_files.contains(s_path) {
                            bundle_files.push(s_path.clone());
                            total_size += sub_item.size.unwrap_or(0);
                        }
                    }
                }

                // Pair matching companion .onnx.data shards for all bundled ONNX graphs
                let bundled_bases: Vec<String> = bundle_files
                    .iter()
                    .map(|p| UniversalQuantization::strip_onnx_companion_suffix(p))
                    .collect();
                for data_item in items {
                    let d_path = &data_item.path;
                    let d_parent = Path::new(d_path)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or("");

                    if UniversalQuantization::is_onnx_companion_file(d_path)
                        && (d_parent == parent_dir || d_path.starts_with(parent_dir))
                    {
                        let d_base = UniversalQuantization::strip_onnx_companion_suffix(d_path);
                        if bundled_bases.contains(&d_base) && !bundle_files.contains(d_path) {
                            bundle_files.push(d_path.clone());
                            total_size += data_item.size.unwrap_or(0);
                        }
                    }
                }

                // Add auxiliary metadata
                for mf in metadata_files {
                    if AssetBundler::is_compatible_subfolder_metadata(mf, &item.path) {
                        bundle_files.push(mf.clone());
                    }
                }
                AssetBundler::filter_duplicate_metadata_files(&mut bundle_files, &item.path);

                let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);

                variants.push(HfVariant {
                    revision: None,
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
        variants
    }

    /// Builds a canonical ModelManifest for a Hugging Face variant
    pub fn build_manifest(
        repo_id: &str,
        variant: &HfVariant,
        pipeline_tag: Option<&str>,
        tags: &[String],
    ) -> ModelManifest {
        let classification = UniversalModelClassifier::classify(pipeline_tag, tags);

        let safe_name = repo_id.split('/').next_back().unwrap_or(repo_id);
        let download_url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            repo_id,
            variant.revision.as_deref().unwrap_or("main"),
            variant.primary_file
        );
        let bit_depth = UniversalQuantization::estimate_bit_depth(&variant.quant_tag);
        let parameters = UniversalQuantization::extract_parameters_from_name(
            &variant.filename,
            &variant.quant_tag,
        );
        let sovereign_id = AssetBundler::resolve_sovereign_id(
            repo_id,
            safe_name,
            &variant.filename,
            &variant.quant_tag,
        );

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_onnx_variants_pairs_moonshine_encoder_and_decoders_by_quant() {
        let items = vec![
            HfTreeItem {
                r#type: Some("file".to_string()),
                path: "onnx/encoder_model.onnx".to_string(),
                size: Some(32_000_000),
            },
            HfTreeItem {
                r#type: Some("file".to_string()),
                path: "onnx/decoder_model_merged.onnx".to_string(),
                size: Some(78_000_000),
            },
            HfTreeItem {
                r#type: Some("file".to_string()),
                path: "onnx/encoder_model_quantized.onnx".to_string(),
                size: Some(9_000_000),
            },
            HfTreeItem {
                r#type: Some("file".to_string()),
                path: "onnx/decoder_model_merged_quantized.onnx".to_string(),
                size: Some(21_000_000),
            },
        ];
        let metadata = vec![
            "config.json".to_string(),
            "tokenizer.json".to_string(),
            "preprocessor_config.json".to_string(),
        ];

        let variants = HuggingFaceHub::bundle_onnx_variants(&items, &metadata);
        // Should emit only 2 primary variants (the encoders), NOT 4 separate variants for decoders
        assert_eq!(variants.len(), 2);

        let fp32 = variants
            .iter()
            .find(|v| v.primary_file == "onnx/encoder_model.onnx")
            .expect("missing fp32 encoder variant");
        assert!(fp32
            .all_files
            .contains(&"onnx/decoder_model_merged.onnx".to_string()));
        assert!(!fp32
            .all_files
            .contains(&"onnx/decoder_model_merged_quantized.onnx".to_string()));
        assert!(fp32.all_files.contains(&"config.json".to_string()));

        let quant = variants
            .iter()
            .find(|v| v.primary_file == "onnx/encoder_model_quantized.onnx")
            .expect("missing quantized encoder variant");
        assert!(quant
            .all_files
            .contains(&"onnx/decoder_model_merged_quantized.onnx".to_string()));
        assert!(!quant
            .all_files
            .contains(&"onnx/decoder_model_merged.onnx".to_string()));
    }

    #[test]
    fn test_int8_bundle_leaves_out_uint8_graphs() {
        // onnx-community/whisper-large-v3-turbo: pulling :int8 brought the uint8 decoders too, and
        // the engine then refused the package ("Ambiguous uncached STT decoder graphs")
        let file = |path: &str| HfTreeItem {
            r#type: Some("file".to_string()),
            path: path.to_string(),
            size: Some(1),
        };
        let items = vec![
            file("onnx/encoder_model_int8.onnx"),
            file("onnx/encoder_model_uint8.onnx"),
            file("onnx/decoder_model_int8.onnx"),
            file("onnx/decoder_model_uint8.onnx"),
            file("onnx/decoder_with_past_model_int8.onnx"),
            file("onnx/decoder_with_past_model_uint8.onnx"),
        ];
        let variants = HuggingFaceHub::bundle_onnx_variants(&items, &[]);
        let int8 = variants
            .iter()
            .find(|v| v.primary_file == "onnx/encoder_model_int8.onnx")
            .expect("missing int8 encoder variant");
        assert!(int8.all_files.contains(&"onnx/decoder_model_int8.onnx".to_string()));
        assert!(!int8.all_files.iter().any(|f| f.contains("uint8")), "{:?}", int8.all_files);
        let uint8 = variants
            .iter()
            .find(|v| v.primary_file == "onnx/encoder_model_uint8.onnx")
            .expect("missing uint8 encoder variant");
        assert!(!uint8.all_files.iter().any(|f| f.contains("_int8")), "{:?}", uint8.all_files);
    }
}
