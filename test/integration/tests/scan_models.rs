use std::path::{Path, PathBuf};
use serde_json::{Value, json};
use std::fs;
use engines::models::GgufProber;

#[tokio::test]
async fn scan_and_register_local_models() {
    let cluaiz_dir = dirs::home_dir().unwrap().join(".cluaiz");
    let models_dir = cluaiz_dir.join("models");
    
    let workspace_dir = std::env::current_dir().unwrap().parent().unwrap().parent().unwrap().to_path_buf();
    let registry_path = workspace_dir.join(".cluaiz/engine/config/model_registry.json");
    
    println!("Scanning models directory: {}", models_dir.display());
    
    // Create registry if it doesn't exist
    let mut registry: Value = if registry_path.exists() {
        let content = fs::read_to_string(&registry_path).unwrap();
        serde_json::from_str(&content).unwrap_or(json!({ "installed_models": {} }))
    } else {
        json!({ "installed_models": {} })
    };
    
    // Clear out old installed models so we only have valid ones
    registry["installed_models"] = json!({});
    
    let categories = vec!["audio", "chat", "embedding", "vision"];
    let mut models_added = 0;
    
    for category in categories {
        let cat_dir = models_dir.join(category);
        if !cat_dir.exists() || !cat_dir.is_dir() {
            continue;
        }
        
        for entry in fs::read_dir(cat_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            
            if path.is_dir() {
                let model_id = path.file_name().unwrap().to_string_lossy().to_string();
                
                let mut format_type = "unknown".to_string();
                let mut files = Vec::new();
                let mut supported_tasks = Vec::new();
                
                // Scan inner files
                if let Ok(model_files) = fs::read_dir(&path) {
                    for f in model_files {
                        let f = f.unwrap();
                        let meta = f.metadata().unwrap();
                        let f_name = f.file_name().to_string_lossy().to_string();
                        let ext = f.path().extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        
                        // STRICT RULE: Only include .gguf and .onnx files. NO JSON!
                        if ext != "gguf" && ext != "onnx" {
                            continue;
                        }
                        
                        if ext == "gguf" {
                            format_type = "gguf".to_string();
                            
                            // Check if it's the primary GGUF file
                            let is_primary = if f_name.contains("-of-") {
                                f_name.contains("-00001-of-") || f_name.contains("-0001-of-")
                            } else {
                                true
                            };
                            
                            files.push(json!({
                                "name": f_name,
                                "size_bytes": meta.len(),
                                "is_primary": is_primary
                            }));
                            
                            // Try to probe the header for supported tasks
                            if is_primary {
                                if let Ok((metadata, _, _)) = GgufProber::probe(&f.path()) {
                                    let arch = metadata.get("general.architecture").map(|s| s.as_str()).unwrap_or("");
                                    let mut tasks = vec!["text-generation", "chat-completion"]; // Base tasks for GGUF LLMs
                                    
                                    if arch == "whisper" {
                                        tasks = vec!["automatic-speech-recognition", "text-to-speech"];
                                    } else if arch.contains("qwen2_vl") || arch.contains("qwen3vl") || arch.contains("llava") || arch.contains("minicpmv") || arch.contains("mllama") || arch.contains("gemma4") {
                                        tasks.push("vision");
                                        tasks.push("image-to-text");
                                    } else if arch.contains("bert") || arch.contains("nomic") {
                                        tasks = vec!["feature-extraction", "embedding"];
                                    }
                                    
                                    // If a model is Gemma or Qwen and has multimodal capabilities via projectors or specific metadata
                                    if metadata.contains_key("general.projector_architecture") || metadata.contains_key("qwen2_vl.vision.patch_size") {
                                        if !tasks.contains(&"vision") {
                                            tasks.push("vision");
                                            tasks.push("image-to-text");
                                        }
                                    }
                                    
                                    supported_tasks = tasks.iter().map(|s| s.to_string()).collect();
                                }
                            }
                            
                        } else if ext == "onnx" {
                            format_type = "onnx".to_string();
                            
                            files.push(json!({
                                "name": f_name,
                                "size_bytes": meta.len(),
                                "is_primary": true // ONNX files are typically standalone or primary graph
                            }));
                        }
                    }
                }
                
                // If we didn't extract supported_tasks from GGUF header, use folder heuristic for ONNX
                if supported_tasks.is_empty() {
                    supported_tasks = match category {
                        "chat" => vec!["text-generation", "chat-completion"],
                        "audio" => vec!["automatic-speech-recognition", "text-to-speech"],
                        "vision" => vec!["vision", "image-to-text"],
                        "embedding" => vec!["feature-extraction", "embedding"],
                        _ => vec![]
                    }.iter().map(|s| s.to_string()).collect();
                }
                
                let hf_repo = if model_id.contains("--") {
                    model_id.replacen("--", "/", 1)
                } else {
                    model_id.clone()
                };
                
                let entry_json = json!({
                    "id": model_id,
                    "category": category,
                    "format_type": format_type,
                    "huggingface_repo": hf_repo,
                    "local_dir": path.to_string_lossy().to_string(),
                    "files": files,
                    "supported_tasks": supported_tasks
                });
                
                registry["installed_models"][&model_id] = entry_json;
                println!("✅ Added model: {} (Format: {}, Tasks: {:?})", model_id, format_type, supported_tasks);
                models_added += 1;
            }
        }
    }
    
    // Save to registry
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&registry_path, serde_json::to_string_pretty(&registry).unwrap()).unwrap();
    println!("🎉 Successfully scanned and added {} models to the registry (Strict Mode)!", models_added);
}
