use std::path::{Path, PathBuf};
use std::fs;
use tracing::{info, warn};
use crate::neural_foundry::registry::SkillManifest;
use crate::neural_foundry::security::permission_schema::PermissionSchema;
use neural_core::interfaces::router_contract::EmbeddingDriver;

use tokio::sync::mpsc;
use lazy_static::lazy_static;

lazy_static! {
    static ref COMPILER_QUEUE: mpsc::UnboundedSender<(PathBuf, SkillManifest)> = {
        let (tx, mut rx) = mpsc::unbounded_channel::<(PathBuf, SkillManifest)>();
        
        tokio::spawn(async move {
            // Delay compilation to ensure primary Chat Engine claims VRAM first
            tokio::time::sleep(tokio::time::Duration::from_secs(8)).await;
            tracing::info!("🛠️ [Compiler Daemon Worker] Background queue active. Processing skills sequentially...");
            
            while let Some((skill_path, manifest)) = rx.recv().await {
                process_skill(skill_path, manifest).await;
            }
        });
        
        tx
    };
}

async fn process_skill(skill_path: PathBuf, manifest: SkillManifest) {
    let permissions = PermissionSchema::load();
    let embedding_model_id = permissions.get_active_embedding_model();
    let gen_model_id = permissions.get_active_chat_model();

    if embedding_model_id.is_none() && gen_model_id.is_none() {
        return;
    }

    let cache_dir = skill_path.join(".cache");
    if !cache_dir.exists() {
        let _ = fs::create_dir_all(&cache_dir);
    }

    let skill_md_path = skill_path.join("SKILL.md");
    if !skill_md_path.exists() {
        return;
    }

    let skill_name = manifest.name.clone();
    info!("🛠️ [Compiler Daemon] Processing Dual-Cache for skill: {}", skill_name);
    
    let skill_content = if let Some(fm) = extract_frontmatter(&skill_path) {
        fm
    } else {
        let semantic_triggers = manifest.triggers.semantic.join(", ");
        format!(
            "Skill Name: {}\nDescription: {}\nTriggers: {}",
            skill_name, manifest.description, semantic_triggers
        )
    };

    // 1. Build Router Embedding Cache
    if let Some(orig_model_id) = embedding_model_id {
        let safe_filename = orig_model_id.replace(":", "-");
        let embedding_cache_path = cache_dir.join(format!("{}.emb.safetensors", safe_filename));
        if !embedding_cache_path.exists() {
            info!("⏳ [Compiler Daemon] Generating Real Router Embedding Cache for {}...", skill_name);
            let roster = crate::models::registry::CoreRoster::load_roster();
            let mut success = false;
            if let Some(model_manifest) = roster.iter().find(|m| m.id == orig_model_id) {
                if let Some(local_path) = &model_manifest.local_path {
                    let model_dir = Path::new(local_path);
                    let model_file = model_dir.join("model.onnx");
                    let tokenizer_file = model_dir.join("tokenizer.json");
                    if model_file.exists() && tokenizer_file.exists() {
                        if let Ok(mut engine) = cluaiz_onnx::engine::OnnxEngine::new() {
                            if engine.load_text_model(&model_file.to_string_lossy(), &tokenizer_file.to_string_lossy(), None).is_ok() {
                                let mut combined_vec = Vec::new();
                                if manifest.triggers.semantic.is_empty() {
                                    if let Ok(vec) = engine.gen_embedding(&manifest.name) {
                                        combined_vec.extend_from_slice(&vec);
                                    }
                                } else {
                                    for trigger in &manifest.triggers.semantic {
                                        if let Ok(vec) = engine.gen_embedding(trigger) {
                                            combined_vec.extend_from_slice(&vec);
                                        }
                                    }
                                }

                                if !combined_vec.is_empty() {
                                    let data_bytes = unsafe { std::slice::from_raw_parts(combined_vec.as_ptr() as *const f32 as *const u8, combined_vec.len() * 4) };
                                    
                                    if let Ok(view) = safetensors::tensor::TensorView::new(safetensors::tensor::Dtype::F32, vec![combined_vec.len()], data_bytes) {
                                        if let Err(e) = safetensors::serialize_to_file(vec![("embedding", view)], None::<std::collections::HashMap<String, String>>, &embedding_cache_path) {
                                            warn!("❌ Failed to write safetensors embedding: {}", e);
                                        } else { 
                                            info!("✅ Real Router Embedding generated: {:?}", embedding_cache_path);
                                            success = true;
                                        // Update GLOBAL_SKILL_ROUTER in memory
                                        if let Ok(mut skill_router) = cluaiz_shared::skills::router::GLOBAL_SKILL_ROUTER.write() {
                                            let norm_skill_path = cluaiz_shared::skills::router::normalize_path(&skill_path);
                                            skill_router.skill_vectors.insert(norm_skill_path, combined_vec);
                                        }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub struct CompilerDaemon;

impl CompilerDaemon {
    pub fn new() -> Self {
        Self
    }

    /// Submits a compilation task to the background global worker queue.
    pub fn compile_skill(&self, skill_path: &Path, manifest: &SkillManifest) {
        let _ = COMPILER_QUEUE.send((skill_path.to_path_buf(), manifest.clone()));
    }
}

fn extract_frontmatter(skill_dir: &std::path::Path) -> Option<String> {
    let skill_md_path = skill_dir.join("SKILL.md");
    if skill_md_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
            let lines: Vec<&str> = content.lines().collect();
            let mut start_idx = None;
            let mut end_idx = None;
            for (i, line) in lines.iter().enumerate() {
                if line.trim() == "---" {
                    if start_idx.is_none() {
                        start_idx = Some(i);
                    } else {
                        end_idx = Some(i);
                        break;
                    }
                }
            }
            if let (Some(start), Some(end)) = (start_idx, end_idx) {
                if end > start + 1 {
                    let frontmatter_lines = &lines[start + 1..end];
                    return Some(frontmatter_lines.join("\n"));
                }
            }
        }
    }
    None
}
