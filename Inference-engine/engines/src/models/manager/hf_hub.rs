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
    pub filename: String,
    pub size_gb: f64,
}

pub struct HuggingFaceHub;

impl HuggingFaceHub {
    /// List all supported model variants (GGUF or ONNX) in a repository
    pub async fn list_variants(repo_id: &str) -> Result<Vec<HfVariant>, String> {
        let client = Client::new();
        let url = format!("https://huggingface.co/api/models/{}/tree/main?recursive=true", repo_id);
        
        let response = client.get(&url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Failed to fetch repository '{}'. Does it exist?", repo_id));
        }

        let items: Vec<HfTreeItem> = response.json().await.map_err(|e| e.to_string())?;
        
        let mut variants = Vec::new();
        for item in &items {
            if item.path.ends_with(".gguf") || item.path.ends_with(".onnx") || item.path.ends_with(".safetensors") || item.path.ends_with(".bin") || item.path.ends_with(".pt") || item.path.ends_with(".awq") {
                let mut total_size = item.size.unwrap_or(0);
                
                // If it's an ONNX file, check if there's a corresponding _data file and add its size
                if item.path.ends_with(".onnx") {
                    let data_path = format!("{}_data", item.path);
                    if let Some(data_item) = items.iter().find(|i| i.path == data_path) {
                        total_size += data_item.size.unwrap_or(0);
                    }
                }
                
                let size_gb = total_size as f64 / (1024.0 * 1024.0 * 1024.0);
                variants.push(HfVariant {
                    filename: item.path.clone(),
                    size_gb,
                });
            }
        }

        if variants.is_empty() {
            return Err(format!("No supported model files (.gguf, .onnx, .safetensors, etc.) found in repository '{}'.", repo_id));
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
        let sovereign_id = format!("{}:{}:{}:{}", family, size, format_ext, quant);

        // 🧠 Intelligent Categorization via HuggingFace API
        let mut is_embedding = false;
        let mut is_vision = false;
        let mut is_image_gen = false;
        let mut is_audio = false;
        
        let client = Client::new();
        let api_url = format!("https://huggingface.co/api/models/{}", repo_id);
        if let Ok(resp) = client.get(&api_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(pipeline_tag) = json.get("pipeline_tag").and_then(|v| v.as_str()) {
                    match pipeline_tag {
                        "feature-extraction" | "sentence-similarity" => is_embedding = true,
                        "image-classification" | "object-detection" | "image-to-text" | "zero-shot-image-classification" | "image-text-to-text" => is_vision = true,
                        "text-to-image" | "image-to-image" => is_image_gen = true,
                        "text-to-speech" | "automatic-speech-recognition" | "audio-classification" | "text-to-audio" | "voice-activity-detection" => is_audio = true,
                        _ => {}
                    }
                }
                
                if let Some(tags) = json.get("tags").and_then(|v| v.as_array()) {
                    for tag in tags {
                        if let Some(tag_str) = tag.as_str() {
                            let tag_lower = tag_str.to_lowercase();
                            if tag_lower == "clip" || tag_lower == "vision" || tag_lower.contains("image") {
                                is_vision = true;
                            }
                            if tag_lower.contains("embedding") || tag_lower.contains("sentence-transformers") {
                                is_embedding = true;
                            }
                            if tag_lower.contains("diffusion") || tag_lower.contains("sdxl") || tag_lower == "text-to-image" {
                                is_image_gen = true;
                            }
                            if tag_lower.contains("audio") || tag_lower.contains("speech") || tag_lower.contains("tts") || tag_lower.contains("whisper") {
                                is_audio = true;
                            }
                        }
                    }
                }
            }
        }

        
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
            architecture: if is_onnx { "ONNX Graph".to_string() } else { "Unknown (GGUF)".to_string() },
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
