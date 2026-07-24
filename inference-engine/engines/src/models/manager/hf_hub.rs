use reqwest::Client;
use serde::Deserialize;
use crate::models::registry::ModelManifest;

#[derive(Debug, Deserialize)]
struct HfTreeItem {
    path: String,
    size: Option<u64>,
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
    /// List all supported model variants (GGUF, ONNX, SafeTensors) in a repository grouped into cohesive bundles
    pub async fn list_variants(repo_id: &str) -> Result<Vec<HfVariant>, String> {
        let client = Client::new();
        let url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=true", repo_id);
        
        let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Failed to fetch repository '{}'. Does it exist?", repo_id));
        }

        let items: Vec<HfTreeItem> = response.json().await.map_err(|e| e.to_string())?;
        
        // Harvest all global and subfolder metadata config JSONs / auxiliary text files
        let metadata_files: Vec<String> = items.iter().filter_map(|item| {
            let path = &item.path;
            let lower = path.to_lowercase();
            if lower.ends_with(".json") || lower.ends_with(".txt") || lower.ends_with("vocab.json") || lower.ends_with("merges.txt") || lower.ends_with("cluaiz-engine.ready") {
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

            if lower.ends_with(".gguf") {
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
                    // e.g. for primary_file "Q4_K_M/cuda/decoder/model.onnx":
                    // - include root-level JSONs like "config.json"
                    // - include path JSONs like "Q4_K_M/cuda/tokenizer.json"
                    // - skip sibling/other path JSONs like "Q4_K_M/default/tokenizer.json"
                    for meta in &metadata_files {
                        if !bundle_files.contains(meta) {
                            if is_directory_prefix(meta, &primary_file) {
                                bundle_files.push(meta.clone());
                            }
                        }
                    }

                    // Extract precision/quant tag from path
                    let quant_tag = extract_quant_tag(&primary_file);
                    let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                    let shard_count = bundle_files.iter().filter(|f| f.ends_with(".gguf")).count();
                    
                    let variant_id = if shard_count > 1 {
                        format!("GGUF {} ({} Shards)", quant_tag, shard_count)
                    } else {
                        format!("GGUF {}", quant_tag)
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
                // Group ONNX models by precision tag / sub-directory
                let quant_tag = extract_quant_tag(path);
                let parent_dir = std::path::Path::new(path).parent().and_then(|p| p.to_str()).unwrap_or("");
                
                let mut bundle_files = Vec::new();
                let mut total_size = 0u64;

                for onnx_item in &items {
                    let o_path = &onnx_item.path;
                    let o_lower = o_path.to_lowercase();
                    let o_parent = std::path::Path::new(o_path).parent().and_then(|p| p.to_str()).unwrap_or("");

                    if o_parent == parent_dir && (o_lower.ends_with(".onnx") || o_lower.ends_with(".onnx_data") || o_lower.ends_with(".onnx.data")) {
                        let o_quant = extract_quant_tag(o_path);
                        if o_quant == quant_tag || (quant_tag == "DEFAULT" && o_path.contains(std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or(""))) {
                            bundle_files.push(o_path.clone());
                            total_size += onnx_item.size.unwrap_or(0);
                            processed_paths.insert(o_path.clone());
                        }
                    }
                }

                if !bundle_files.is_empty() {
                    bundle_files.sort();
                    let primary_file = bundle_files.iter()
                        .find(|f| f.contains("decoder") || f.contains("model.onnx"))
                        .cloned()
                        .unwrap_or_else(|| bundle_files[0].clone());

                    // Only attach metadata JSONs matching this ONNX variant's path prefix
                    // e.g. Q4_K_M/cuda variant only gets Q4_K_M/cuda/*.json + root-level JSONs, NOT default/*.json or NF4/*.json
                    for meta in &metadata_files {
                        if !bundle_files.contains(meta) {
                            if is_directory_prefix(meta, &primary_file) {
                                bundle_files.push(meta.clone());
                            }
                        }
                    }

                    let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                    let variant_id = format!("ONNX {} ({})", quant_tag, parent_dir);

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
        
        let name_to_process = if name == "model" || name == "pytorch_model" { fallback.clone() } else { name };
        
        let mut family_parts = Vec::new();
        let mut size = "unknown".to_string();
        let mut quant = "unknown".to_string();
        
        if is_onnx {
            quant = "fp32".to_string(); // Default assumption for ONNX unless specified
        }

        let parts: Vec<&str> = name_to_process.split('-').collect();
        let mut found_size = false;

        for part in parts {
            if (part.starts_with('q') && part.chars().nth(1).map_or(false, |c| c.is_digit(10))) 
               || part == "f16" || part == "f32" || part == "int4" || part == "int8" || part == "fp32" {
                quant = part.to_string();
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
        let sovereign_id = repo_id.split('/').next_back().unwrap_or(repo_id).to_string();

        // 🧠 Intelligent Categorization via HuggingFace API
        let mut is_embedding = false;
        let mut is_vision = false;
        let mut is_image_gen = false;
        let mut is_audio = false;
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

                if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                    match pipeline_tag {
                        "feature-extraction" | "sentence-similarity" => is_embedding = true,
                        "image-classification" | "object-detection" | "image-to-text" | "zero-shot-image-classification" | "image-text-to-text" => is_vision = true,
                        "text-to-image" | "image-to-image" => is_image_gen = true,
                        "text-to-speech" | "automatic-speech-recognition" | "audio-classification" | "text-to-audio" | "voice-activity-detection" => is_audio = true,
                        _ => {}
                    }
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
        
        let category = if is_embedding {
            "embedding".to_string()
        } else if is_image_gen {
            "image_gen".to_string()
        } else if is_vision {
            "vision".to_string()
        } else if is_audio {
            "audio".to_string()
        } else {
            "chat".to_string()
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

fn extract_quant_tag(path: &str) -> String {
    let path_obj = std::path::Path::new(path);
    
    // 1. Check parent folder name first if path is inside a subfolder (excluding generic directory names)
    if let Some(parent) = path_obj.parent().and_then(|p| p.to_str()) {
        if !parent.is_empty() && parent != "." {
            let p_lower = parent.to_lowercase();
            if p_lower != "onnx" && p_lower != "models" && p_lower != "weights" {
                if p_lower.contains('q') || p_lower.contains("bf16") || p_lower.contains("fp16") || p_lower.contains("int") || p_lower.contains("ud") {
                    return parent.to_string();
                }
            }
        }
    }

    // 2. Scan filename tokens for compound quant tags (e.g. UD-Q2_K_XL, Q4_K_M, Q8_0, Q3_K_S)
    let file_name = path_obj.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let lower = file_name.to_lowercase();
    
    let parts: Vec<&str> = lower.split(&['/', '\\', '.', ' '][..]).collect();
    for part in parts.iter().rev() {
        if part.starts_with("ud-q") || part.starts_with("q4_") || part.starts_with("q5_") || part.starts_with("q8_") || part.starts_with("q2_") || part.starts_with("q3_") || part.starts_with("q6_") {
            return part.to_uppercase();
        }
    }

    let sub_parts: Vec<&str> = lower.split(&['/', '\\', '-', '_', '.'][..]).collect();
    for part in sub_parts.iter().rev() {
        if (part.starts_with('q') || part.starts_with("iq")) && part.len() >= 2 && part.chars().nth(1).map_or(false, |c| c.is_digit(10)) {
            return part.to_uppercase();
        }
        if *part == "bf16" || *part == "fp16" || *part == "fp32" || *part == "int8" || *part == "uint8" || *part == "int4" || *part == "q4f16" {
            return part.to_uppercase();
        }
    }

    "DEFAULT".to_string()
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

