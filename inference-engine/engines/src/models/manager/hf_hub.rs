use reqwest::Client;
use serde::Deserialize;
use crate::models::registry::ModelManifest;
use crate::models::fetch::tts_resolver::TtsAssetResolver;
use crate::models::fetch::AssetResolver;

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
    /// Fetch raw file tree items from HuggingFace repository
    pub async fn list_raw_tree(repo_id: &str) -> Result<Vec<HfTreeItem>, String> {
        let client = Client::new();
        let mut url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=true", repo_id);
        let mut items = Vec::new();

        loop {
            let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("Failed to fetch repository '{}'. Does it exist?", repo_id));
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

    /// List all supported model variants (GGUF, ONNX, SafeTensors) in a repository grouped into cohesive bundles
    pub async fn list_variants(repo_id: &str) -> Result<Vec<HfVariant>, String> {
        let items = Self::list_raw_tree(repo_id).await?;
        
        // Harvest all global and subfolder metadata config JSONs / auxiliary text files & binary style assets
        let metadata_files: Vec<String> = items.iter().filter_map(|item| {
            let path = &item.path;
            let lower = path.to_lowercase();
            if lower.ends_with(".json") 
                || lower.ends_with(".txt") 
                || lower.ends_with(".yaml")
                || lower.ends_with("vocab.json") 
                || lower.ends_with("merges.txt") 
                || lower.ends_with("cluaiz-engine.ready")
                || lower.ends_with(".onnx.data")
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

        let mut variants: Vec<HfVariant> = Vec::new();
        let mut processed_paths = std::collections::HashSet::new();

        for item in &items {
            if processed_paths.contains(&item.path) {
                continue;
            }

            let path = &item.path;
            let lower = path.to_lowercase();

            if lower.ends_with(".gguf") && !crate::models::fetch::asset_resolver::AssetResolver::is_helper_gguf(&lower) {
                // Check if this is a sharded GGUF file (e.g. 00001-of-00033.gguf)
                let is_shard = path.contains("-00001-of-") || path.contains("_00001-of-");
                let shard_match = if is_shard {
                    if let Some(idx) = path.find("-00001-of-").or_else(|| path.find("_00001-of-")) {
                        Some(&path[..idx])
                    } else {
                        None
                    }
                } else {
                    None
                };

                let mut bundle_files = Vec::new();
                let mut total_size = 0u64;

                if let Some(prefix) = shard_match {
                    // Collect all matching shards for this GGUF variant
                    for shard_item in &items {
                        if shard_item.path.starts_with(prefix) && shard_item.path.ends_with(".gguf") {
                            bundle_files.push(shard_item.path.clone());
                            total_size += shard_item.size.unwrap_or(0);
                            processed_paths.insert(shard_item.path.clone());
                        }
                    }
                } else if path.ends_with(".gguf") {
                    bundle_files.push(path.clone());
                    total_size += item.size.unwrap_or(0);
                    processed_paths.insert(path.clone());
                }

                if !bundle_files.is_empty() {
                    bundle_files.sort();
                    let primary_file = bundle_files[0].clone();
                    
                    // Only attach metadata JSONs that are directories-prefix compatible with the primary model file
                    for meta in &metadata_files {
                        if !bundle_files.contains(meta) {
                            if is_directory_prefix(meta, &primary_file) {
                                bundle_files.push(meta.clone());
                            }
                        }
                    }

                    // ── Smart Helper GGUF Bundling ────────────────────────────────────────
                    // All detection + selection logic lives in AssetResolver (Single Source of Truth).
                    // hf_hub.rs only orchestrates — it never duplicates string-matching patterns.
                    use crate::models::fetch::asset_resolver::AssetResolver;

                    let main_quant = AssetResolver::extract_quant_tag(&primary_file);

                    // Collect all repo helper GGUFs, split by type via AssetResolver
                    let all_helpers: Vec<&HfTreeItem> = items.iter()
                        .filter(|i| i.path.to_lowercase().ends_with(".gguf")
                            && AssetResolver::is_helper_gguf(&i.path))
                        .collect();

                    // MTP: select single best match for main model's quant
                    let mtp_paths: Vec<&str> = all_helpers.iter()
                        .filter(|i| AssetResolver::is_mtp_gguf(&i.path))
                        .map(|i| i.path.as_str())
                        .collect();
                    if let Some(best_mtp) = AssetResolver::select_best_mtp(&mtp_paths, &main_quant) {
                        if !bundle_files.contains(&best_mtp.to_string()) {
                            bundle_files.push(best_mtp.to_string());
                        }
                    }

                    // mmproj: select single best quality (F16 > BF16 > F32)
                    let mmproj_paths: Vec<&str> = all_helpers.iter()
                        .filter(|i| AssetResolver::is_mmproj_gguf(&i.path))
                        .map(|i| i.path.as_str())
                        .collect();
                    if let Some(best_mmproj) = AssetResolver::select_best_mmproj(&mmproj_paths) {
                        if !bundle_files.contains(&best_mmproj.to_string()) {
                            bundle_files.push(best_mmproj.to_string());
                        }
                    }

                    // Other helpers (projector-*, adapter-*): bundle all — usually single file
                    for helper in &all_helpers {
                        if !AssetResolver::is_mtp_gguf(&helper.path)
                            && !AssetResolver::is_mmproj_gguf(&helper.path)
                            && !bundle_files.contains(&helper.path)
                        {
                            bundle_files.push(helper.path.clone());
                        }
                    }

                    // Frontend assets (ONNX hybrid models, voice assets)
                    for any_item in &items {
                        let path_lower = any_item.path.to_lowercase();
                        let is_frontend = path_lower.contains("frontend-onnx/")
                            || path_lower.contains("voices/")
                            || path_lower.contains("voice_styles/")
                            || path_lower.contains("espeak-ng-data/")
                            || path_lower.ends_with(".yaml")
                            || path_lower.ends_with(".yml");
                        if is_frontend && !bundle_files.contains(&any_item.path) {
                            bundle_files.push(any_item.path.clone());
                        }
                    }



                    filter_duplicate_metadata_files(&mut bundle_files);

                    // Extract precision/quant tag from path
                    let quant_tag = AssetResolver::extract_quant_tag(&primary_file);
                    let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                    let shard_count = bundle_files.iter().filter(|f| f.ends_with(".gguf")).count();
                    
                    let file_stem = std::path::Path::new(&primary_file)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let variant_id = if shard_count > 1 {
                        format!("GGUF {} ({}) ({} Shards)", quant_tag, file_stem, shard_count)
                    } else {
                        format!("GGUF {} ({})", quant_tag, file_stem)
                    };

                    variants.push(HfVariant {
                        variant_id,
                        format_type: "gguf".to_string(),
                        quant_tag,
                        primary_file: primary_file.clone(),
                        all_files: bundle_files,
                        filename: primary_file,
                        size_gb,
                    });
                }
            } else if lower.ends_with(".onnx") {
                // SUBCOMPONENT ENTRYPOINT GATING: Do not create standalone variants for sub-graphs (vocoder, denoiser, tokenizer)
                // if a primary backbone model exists, OR if another subcomponent already created the unified pipeline variant.
                let parent_dir = std::path::Path::new(path).parent().and_then(|p| p.to_str()).unwrap_or("");
                let is_subcomp = TtsAssetResolver::is_subcomponent_file(path);

                if is_subcomp {
                    // Check if there is a primary entrypoint file in the same directory
                    let has_primary = items.iter().any(|i| {
                        let i_lower = i.path.to_lowercase();
                        let i_dir = std::path::Path::new(&i.path).parent().and_then(|p| p.to_str()).unwrap_or("");
                        i_dir == parent_dir && i_lower.ends_with(".onnx") && !TtsAssetResolver::is_subcomponent_file(&i.path)
                    });

                    if has_primary {
                        continue; // Skip creating standalone variant — this subcomponent will be bundled into the primary model variant
                    }

                    // If all files in the directory are subcomponents (split pipeline repo), only allow the first ONNX file to anchor the unified pipeline
                    let first_subcomp = items.iter().find(|i| {
                        let i_lower = i.path.to_lowercase();
                        let i_dir = std::path::Path::new(&i.path).parent().and_then(|p| p.to_str()).unwrap_or("");
                        i_dir == parent_dir && i_lower.ends_with(".onnx")
                    });
                    if let Some(first) = first_subcomp {
                        if first.path != *path {
                            continue; // Skip duplicate anchor — first subcomponent already gathered all pipeline stages into 1 bundle
                        }
                    }
                }

                // Group ONNX models by precision tag / sub-directory & attach FP16 companion vocoders
                let quant_tag = AssetResolver::extract_quant_tag(path);
                let is_combined_file = lower.contains("combined");
                
                // If the folder is deep and contains split pipeline stages (flow/, llm/, hift/), we identify the base directory of the pipeline.
                let pipeline_base_dir = if parent_dir.to_lowercase().ends_with("/llm") 
                    || parent_dir.to_lowercase().ends_with("/flow") 
                    || parent_dir.to_lowercase().ends_with("/hift") 
                {
                    std::path::Path::new(parent_dir).parent().and_then(|p| p.to_str()).unwrap_or(parent_dir)
                } else {
                    parent_dir
                };

                let mut bundle_files = Vec::new();
                let mut total_size = 0u64;

                // Pass 1: Collect ONNX Graph files (.onnx only)
                for onnx_item in &items {
                    let o_path = &onnx_item.path;
                    let o_lower = o_path.to_lowercase();
                    let o_parent = std::path::Path::new(o_path).parent().and_then(|p| p.to_str()).unwrap_or("");
                    let o_combined = o_lower.contains("combined");

                    let in_same_pipeline = o_parent == parent_dir || (!pipeline_base_dir.is_empty() && o_path.starts_with(pipeline_base_dir));

                    if in_same_pipeline && o_lower.ends_with(".onnx") {
                        // Enforce Combined Graph vs Split Graph Mutual Exclusion
                        if is_combined_file && !o_combined && (o_lower.contains("flow") || o_lower.contains("hift") || o_lower.contains("vocoder")) {
                            continue; // Skip isolated split graphs if user target is combined graph
                        }
                        if !is_combined_file && o_combined {
                            continue; // Skip combined graph if user target is split graphs
                        }

                        // SCALE ISOLATION: Prevent mixing base/ vs base_small/, custom/ vs custom_small/ etc.
                        if TtsAssetResolver::is_scale_mismatch(path, o_path) {
                            continue; // Different architecture scales — do NOT bundle together
                        }

                        // ANTI-REDUNDANCY: Skip files that are the same model graph but different quantizations
                        // e.g. model.onnx vs model_q4.onnx vs model_fp16.onnx should be SEPARATE variants
                        if TtsAssetResolver::is_same_model_different_quant(path, o_path) {
                            continue;
                        }

                        let o_quant = AssetResolver::extract_quant_tag(o_path);
                        let is_subcomp = TtsAssetResolver::is_subcomponent_file(o_path);
                        
                        let matches_quant = o_quant == quant_tag;
                        let is_fallback_default = o_quant == "DEFAULT" && !items.iter().any(|i| {
                            let i_quant = AssetResolver::extract_quant_tag(&i.path);
                            i_quant == quant_tag && TtsAssetResolver::is_same_model_different_quant(o_path, &i.path)
                        });

                        if matches_quant || is_subcomp || is_fallback_default {
                            bundle_files.push(o_path.clone());
                            total_size += onnx_item.size.unwrap_or(0);
                            processed_paths.insert(o_path.clone());
                        }
                    }
                }

                // Pass 2: Collect companion weights files (.onnx_data, .onnx.data, .data)
                // only if their corresponding .onnx file is in bundle_files
                let strip_onnx_extension_only = |p: &str| -> String {
                    let p_lower = p.to_lowercase();
                    let suffixes = [".onnx.data", ".onnx_data", ".onnx", ".data"];
                    for suffix in &suffixes {
                        if p_lower.ends_with(suffix) {
                            return p[..p.len() - suffix.len()].to_string();
                        }
                    }
                    p.to_string()
                };

                for data_item in &items {
                    let d_path = &data_item.path;
                    let d_lower = d_path.to_lowercase();
                    let d_parent = std::path::Path::new(d_path).parent().and_then(|p| p.to_str()).unwrap_or("");
                    
                    let in_same_pipeline = d_parent == parent_dir || (!pipeline_base_dir.is_empty() && d_path.starts_with(pipeline_base_dir));
                    let is_companion_data = d_lower.ends_with(".onnx.data") || d_lower.ends_with(".onnx_data") || d_lower.ends_with(".data");
                    
                    if in_same_pipeline && is_companion_data {
                        // Resolve the corresponding model file path by matching base path
                        let d_clean = strip_onnx_extension_only(d_path);
                        let has_matching_graph = bundle_files.iter().any(|b_path| {
                            b_path.ends_with(".onnx") && strip_onnx_extension_only(b_path) == d_clean
                        });
                        
                        if has_matching_graph {
                            bundle_files.push(d_path.clone());
                            total_size += data_item.size.unwrap_or(0);
                            processed_paths.insert(d_path.clone());
                        }
                    }
                }

                if !bundle_files.is_empty() {
                    bundle_files.sort();
                    let primary_file = bundle_files.iter()
                        .find(|f| f.contains("slow_ar") || f.contains("kokoro") || f.contains("generator") || f.contains("speech_llm") || f.contains("decoder") || f.contains("model.onnx") || f.contains("model_uint8") || f.contains("model_q4") || f.contains("model_int8") || f.contains("model_quantized"))
                        .cloned()
                        .unwrap_or_else(|| bundle_files[0].clone());

                    // Attach metadata JSONs, YAMLs, and binary voice directories matching directory prefix
                    for meta in &metadata_files {
                        if !bundle_files.contains(meta) {
                            let meta_lower = meta.to_lowercase();
                            let is_root_meta = !meta.contains('/');
                            let in_pipeline_path = !pipeline_base_dir.is_empty() && meta.starts_with(pipeline_base_dir);
                            let is_voice_or_frontend_asset = meta_lower.ends_with(".onnx.data") || meta_lower.contains("voices/") || meta_lower.contains("voice_styles/") || meta_lower.contains("vocoder/") || meta_lower.contains("codec/") || meta_lower.contains("espeak-ng-data/") || meta_lower.contains("frontend-onnx/");

                            if is_root_meta || in_pipeline_path || is_voice_or_frontend_asset {
                                if !bundle_files.contains(meta) {
                                    bundle_files.push(meta.clone());
                                }
                            }
                        }
                    }

                    filter_duplicate_metadata_files(&mut bundle_files);

                    let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                    let label_dir = if !pipeline_base_dir.is_empty() { pipeline_base_dir } else { parent_dir };
                    let file_stem = std::path::Path::new(&primary_file)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let variant_id = if !label_dir.is_empty() && label_dir != "." {
                        format!("ONNX {} ({}/{})", quant_tag, label_dir, file_stem)
                    } else {
                        format!("ONNX {} ({})", quant_tag, file_stem)
                    };

                    variants.push(HfVariant {
                        variant_id,
                        format_type: "onnx".to_string(),
                        quant_tag,
                        primary_file: primary_file.clone(),
                        all_files: bundle_files,
                        filename: primary_file,
                        size_gb,
                    });
                }
            }
        }

        if variants.is_empty() {
            return Err(format!("No supported model files (.gguf, .onnx) found in repository '{}'.", repo_id));
        }

        Ok(variants)
    }

    pub async fn build_manifest(repo_id: &str, filename: &str, download_size_gb: f64) -> Result<ModelManifest, String> {
        let repo_id = if repo_id.contains(':') {
            repo_id.split(':').next().unwrap_or(repo_id)
        } else {
            repo_id
        };

        let url = format!("https://huggingface.co/{}/resolve/main/{}", repo_id, filename);
        
        // Base Engine + Weights overhead (~0.5 GB). KV Cache will dynamically add more.
        let ram_required_gb = download_size_gb + 0.5;

        let format_ext = filename.split('.').last().unwrap_or("unknown");
        let is_onnx = format_ext == "onnx";

        let file_basename = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(filename);

        // Auto-format ID to Sovereign Library standard: family:size:format:quantization
        let name = file_basename.to_lowercase().replace(".gguf", "").replace(".onnx", "").replace(".safetensors", "").replace(".bin", "").replace(".pt", "").replace(".awq", "");
        let fallback = repo_id.split('/').last().unwrap_or(&name).to_lowercase();
        
        let name_to_process = if name == "model" || name == "pytorch_model" { fallback.clone() } else { name.clone() };
        
        let mut family_parts = Vec::new();
        let mut size = "unknown".to_string();
        
        let mut quant = AssetResolver::extract_quant_tag(filename);
        if quant == "DEFAULT" {
            quant = if is_onnx { "fp32".to_string() } else { "unknown".to_string() };
        }

        let parts: Vec<&str> = name_to_process.split('-').collect();
        let mut found_size = false;

        for part in parts {
            let part_upper = part.to_uppercase();
            if (!quant.is_empty() && quant != "unknown" && quant.contains(&part_upper))
               || (part.starts_with('q') && part.chars().nth(1).map_or(false, |c| c.is_digit(10))) 
               || part == "f16" || part == "f32" || part == "int4" || part == "int8" || part == "fp32" {
                continue;
            }

            if part.ends_with('b') && (part.chars().next().unwrap_or('a').is_digit(10) || part.contains('x')) {
                size = part.to_string();
                found_size = true;
                continue;
            }

            if part == "instruct" || part == "chat" {
                family_parts.push(part);
                continue;
            }

            if !found_size && part != "gguf" && part != "onnx" && part != "unsloth" {
                family_parts.push(part);
            }
        }

        let family = if family_parts.is_empty() { fallback.clone() } else { family_parts.join("_") };
        let sovereign_id = AssetResolver::resolve_sovereign_id(repo_id, &name, filename, &quant);

        // 🧠 Intelligent Categorization via HuggingFace API
        let mut has_chat_tags = false;
        let mut hf_pipeline_tag = None;
        let mut raw_architecture = String::new();

        let client = Client::new();
        let api_url = format!("https://huggingface.co/api/models/{}", repo_id);
        if let Ok(resp) = client.get(&api_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                // Try 1: HF API config section
                if let Some(config) = json.get("config") {
                    if let Some(model_type) = config.get("model_type").and_then(|v| v.as_str()) {
                        raw_architecture = model_type.to_string();
                    } else if let Some(archs) = config.get("architectures").and_then(|v| v.as_array()) {
                        if let Some(first_arch) = archs.first().and_then(|v| v.as_str()) {
                            raw_architecture = first_arch.to_string();
                        }
                    }
                }

                // Try 2: Root-level model_type
                if raw_architecture.is_empty() {
                    if let Some(mt) = json.get("model_type").and_then(|v| v.as_str()) {
                        raw_architecture = mt.to_string();
                    }
                }

                if let Some(tags_arr) = json.get("tags").and_then(|t| t.as_array()) {
                    for tag in tags_arr {
                        if let Some(t_str) = tag.as_str() {
                            let t_lower = t_str.to_lowercase();
                            if t_lower == "conversational" || t_lower == "text-generation" || t_lower == "text-generation-inference" || t_lower.contains("instruct") || t_lower.contains("chat") || t_lower == "it" {
                                has_chat_tags = true;
                            }
                        }
                    }
                }

                if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                    hf_pipeline_tag = Some(pipeline_tag.to_string());
                }
            }
        }

        // Try 3: Directly fetch config.json from the repo (same file that post-download prober reads)
        // This is the ground truth for architecture detection — works for ALL model types
        if raw_architecture.is_empty() {
            let config_url = format!("https://huggingface.co/{}/resolve/main/config.json", repo_id);
            if let Ok(resp) = client.get(&config_url).send().await {
                if resp.status().is_success() {
                    if let Ok(config_json) = resp.json::<serde_json::Value>().await {
                        if let Some(mt) = config_json.get("model_type").and_then(|v| v.as_str()) {
                            raw_architecture = mt.to_string();
                        } else if let Some(archs) = config_json.get("architectures").and_then(|v| v.as_array()) {
                            if let Some(first) = archs.first().and_then(|v| v.as_str()) {
                                raw_architecture = first.to_string();
                            }
                        }
                    }
                }
            }
        }

        let architecture = if !raw_architecture.is_empty() {
            let clean = raw_architecture
                .replace("ForCausalLM", "")
                .replace("ForConditionalGeneration", "")
                .replace("ForSpeechSeq2Seq", "")
                .replace("Model", "");
            let mut chars = clean.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Model Graph".to_string(),
            }
        } else if is_onnx {
            "ONNX Graph".to_string()
        } else {
            "GGUF Graph".to_string()
        };
        
        let category = AssetResolver::resolve_category(repo_id, filename, hf_pipeline_tag.as_deref(), has_chat_tags);
        let is_vision = if let Some(ref tag) = hf_pipeline_tag {
            let t = tag.to_lowercase();
            t.contains("image") || t.contains("vision")
        } else {
            false
        };


        Ok(ModelManifest {
            id: sovereign_id.clone(),
            name: sovereign_id.clone(),
            architecture,
            architecture_type: format_ext.to_string(),
            parameters: "Unknown".to_string(),
            training_tokens: "Unknown".to_string(),
            bit_depth: if is_onnx { 32.0 } else { 4.0 }, // Default assumption
            ram_required_gb,
            download_size_gb,
            huggingface_repo: repo_id.to_string(),
            huggingface_filename: file_basename.to_string(),
            download_url: url,
            description: "Custom HuggingFace Model".to_string(),
            is_cloud_api: false,
            requires_gpu: false,
            is_free_tier: true,
            input_modality: if is_vision { "Text + Vision".to_string() } else { "Text".to_string() },
            context_window: "8k".to_string(), // Default assumption, will be updated by prober
            family: fallback,
            category,
            assets: vec![],
            local_path: None,
            dna_path: None,
            has_vision: is_vision,
            has_audio: false,
            expert_count: None,
            experts_per_token: None,
        })
    }

    /// Fetches the first 8MB of a remote GGUF file to probe its metadata
    pub async fn fetch_partial_gguf_metadata(url: &str) -> Result<(std::collections::HashMap<String, String>, std::collections::HashMap<String, Vec<usize>>, usize), String> {
        use reqwest::header::RANGE;
        use std::io::Write;

        // Custom client that DOES NOT follow redirects automatically
        let custom_policy = reqwest::redirect::Policy::none();
        let client = Client::builder().redirect(custom_policy).build().map_err(|e| e.to_string())?;
        
        let mut target_url = url.to_string();
        
        // Manual redirect following (up to 5 times) to preserve headers
        for _ in 0..5 {
            let res = client.get(&target_url).header(RANGE, "bytes=0-8388607").send().await.map_err(|e| e.to_string())?;
            if res.status().is_redirection() {
                if let Some(loc) = res.headers().get(reqwest::header::LOCATION) {
                    target_url = loc.to_str().unwrap_or(&target_url).to_string();
                    continue;
                }
            }
            
            if !res.status().is_success() && res.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(format!("Failed to fetch GGUF header chunks. HTTP {}", res.status()));
            }

            let bytes = res.bytes().await.map_err(|e| e.to_string())?;
            
            let temp_dir = std::env::temp_dir();
            let temp_file_path = temp_dir.join(format!("cluaiz_probe_{}.gguf", std::process::id()));
            
            let mut file = std::fs::File::create(&temp_file_path).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            
            let result = cluaiz_shared::utils::gguf_prober::GGUFProber::probe(&temp_file_path);
            let _ = std::fs::remove_file(&temp_file_path);
            
            return result.map_err(|e| e.to_string());
        }
        
        Err("Too many redirects while trying to fetch metadata".to_string())
    }
}


fn basename_lower(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_lowercase()
}

/// Returns an integer tier for a quant tag (higher = higher quality / larger size).
/// Used to find the MTP file closest in quality to the selected main model quant.
///
/// Tier mapping (approximate, from smallest to largest):
///   0  = unknown/default
///   1  = Q2_K / IQ1 / IQ2
///   2  = Q3_K / IQ3
///   3  = Q4_0 / Q4_K / IQ4
///   4  = Q5_K / Q5_0
///   5  = Q6_K / Q6_0
///   6  = Q8_0 / Q8_K
///   7  = F16 / BF16
///   8  = F32
fn quant_tier_of(quant: &str) -> usize {
    let q = quant.to_lowercase();
    if q.contains("f32") || q.contains("fp32") { return 8; }
    if q.contains("f16") || q.contains("bf16") || q.contains("fp16") { return 7; }
    if q.contains("q8") { return 6; }
    if q.contains("q6") { return 5; }
    if q.contains("q5") { return 4; }
    if q.contains("q4") || q.contains("iq4") || q.contains("ud-q4") { return 3; }
    if q.contains("q3") || q.contains("iq3") { return 2; }
    if q.contains("q2") || q.contains("iq2") || q.contains("iq1") { return 1; }
    0
}

fn is_directory_prefix(meta_path: &str, primary_path: &str) -> bool {
    let meta_dir = match std::path::Path::new(meta_path).parent() {
        Some(p) => p.to_str().unwrap_or(""),
        None => "",
    };
    let primary_dir = match std::path::Path::new(primary_path).parent() {
        Some(p) => p.to_str().unwrap_or(""),
        None => "",
    };
    
    if meta_dir.is_empty() {
        return true; // Root level files are always shared
    }
    
    let meta_dir = meta_dir.replace('\\', "/");
    let primary_dir = primary_dir.replace('\\', "/");
    
    let meta_parts: Vec<&str> = meta_dir.split('/').filter(|s| !s.is_empty()).collect();
    let primary_parts: Vec<&str> = primary_dir.split('/').filter(|s| !s.is_empty()).collect();
    
    if meta_parts.len() > primary_parts.len() {
        return false;
    }
    
    for (m_part, p_part) in meta_parts.iter().zip(primary_parts.iter()) {
        if m_part != p_part {
            return false;
        }
    }
    
    true
}

fn is_compatible_subfolder_metadata(meta_path: &str, primary_path: &str) -> bool {
    let lower = meta_path.to_lowercase();
    let is_asset = lower.ends_with(".json") 
        || lower.ends_with(".txt") 
        || lower.ends_with(".yaml") 
        || lower.ends_with(".yml") 
        || lower.ends_with(".onnx")
        || lower.ends_with(".onnx.data")
        || lower.ends_with(".bin")
        || lower.contains("vocab") 
        || lower.contains("merges") 
        || lower.contains("token")
        || lower.contains("voices");
        
    if !is_asset {
        return false;
    }

    let meta_dir = std::path::Path::new(meta_path).parent().and_then(|p| p.to_str()).unwrap_or("");
    if meta_dir.is_empty() {
        return true;
    }

    let meta_dir_lower = meta_dir.to_lowercase();
    let primary_lower = primary_path.to_lowercase();

    if (meta_dir_lower.contains("q4") || meta_dir_lower.contains("int8") || meta_dir_lower.contains("fp16") || meta_dir_lower.contains("quant")) 
        && !primary_lower.contains(&meta_dir_lower) 
        && !meta_dir_lower.contains("voices")
    {
        return false;
    }

    true
}

fn filter_duplicate_metadata_files(bundle_files: &mut Vec<String>) {
    let mut subfolder_basenames = std::collections::HashSet::new();
    for f in bundle_files.iter() {
        if let Some(parent) = std::path::Path::new(f).parent() {
            if !parent.as_os_str().is_empty() {
                if let Some(basename) = std::path::Path::new(f).file_name().and_then(|s| s.to_str()) {
                    subfolder_basenames.insert(basename.to_lowercase());
                }
            }
        }
    }

    bundle_files.retain(|f| {
        let path = std::path::Path::new(f);
        let parent = path.parent().and_then(|p| p.to_str()).unwrap_or("");
        if parent.is_empty() {
            if let Some(basename) = path.file_name().and_then(|s| s.to_str()) {
                if subfolder_basenames.contains(&basename.to_lowercase()) {
                    return false; // Drop root file if subfolder version already exists!
                }
            }
        }
        true
    });
}


