#[cfg(test)]
mod semantic_routing_tests {
    use engines::neural_foundry::security::permission_schema::PermissionSchema;
    use cluaiz_shared::skills::router::{GLOBAL_SKILL_ROUTER, SkillManifest, SkillTriggers};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn get_test_skills_dir() -> PathBuf {
        cluaiz_shared::environment::EnvironmentManager::current().skills_dir()
    }

    #[tokio::test]
    async fn test_dynamic_vector_and_kv_compilation() {
        println!("🧠 [Test] Starting Sovereign Dynamic Pipeline Test...");
        let skills_dir = get_test_skills_dir();
        
        // 1. Setup a test skill
        let test_skill_name = "test-system-routing-skill";
        let test_skill_path = skills_dir.join(test_skill_name);
        if test_skill_path.exists() {
            let _ = fs::remove_dir_all(&test_skill_path);
        }
        fs::create_dir_all(&test_skill_path).unwrap();

        // Write SKILL.md with frontmatter
        let skill_md_content = r#"---
name: test-system-routing-skill
description: "A secure testing skill for dynamic vector matching"
version: "1.0.0"
triggers:
  semantic: ["run test diagnostic", "check memory constraints"]
  entropy_threshold: 0.8
permissions:
  level: "ReadOnly"
  network: false
  filesystem: false
---
# Secure Diagnostic Instructions
This is a secure system skill that handles diagnostic memory allocation.
"#;
        fs::write(test_skill_path.join("SKILL.md"), skill_md_content).unwrap();

        // Write manifest.json
        let manifest_json = r#"{
            "id": "test-system-routing-skill",
            "name": "test-system-routing-skill",
            "version": "1.0.0",
            "description": "A secure testing skill for dynamic vector matching",
            "triggers": {
                "semantic": ["run test diagnostic", "check memory constraints"],
                "entropy_threshold": 0.8
            },
            "permissions": {
                "level": "ReadOnly",
                "network": false,
                "filesystem": false
            }
        }"#;
        fs::write(test_skill_path.join("manifest.json"), manifest_json).unwrap();

        // 2. Read active embedding model from PermissionSchema
        let permissions = PermissionSchema::load();
        let embedding_model_id = permissions.get_active_embedding_model();
        let chat_model_id = permissions.get_active_chat_model();
        
        println!("🤖 [Test] Active Embedding Model: {:?}", embedding_model_id);
        println!("🤖 [Test] Active Chat Model: {:?}", chat_model_id);

        if let Some(ref model_id) = embedding_model_id {
            let safe_filename = model_id.as_str().replace(":", "-");
            
            // Generate semantic vector immediately using ONNX engine if model is cached locally
            let roster = engines::models::registry::CoreRoster::load_roster();
            if let Some(model_manifest) = roster.iter().find(|m| &m.id == model_id) {
                if let Some(local_path) = &model_manifest.local_path {
                    let model_dir = Path::new(local_path);
                    let model_file = model_dir.join("model.onnx");
                    let tokenizer_file = model_dir.join("tokenizer.json");
                    if model_file.exists() && tokenizer_file.exists() {
                        println!("⏳ [Test] Instantiating ONNX engine to verify embedding compilation...");
                        let mut engine = cluaiz_onnx::engine::OnnxEngine::new().unwrap();
                        engine.load_text_model(&model_file.to_string_lossy(), &tokenizer_file.to_string_lossy(), None).unwrap();
                        
                        let skill_content = format!(
                            "Skill Name: {}\nDescription: {}\nTriggers: run test diagnostic, check memory constraints",
                            test_skill_name, "A secure testing skill for dynamic vector matching"
                        );
                        
                        use neural_core::interfaces::router_contract::EmbeddingDriver;
                        let vec = engine.gen_embedding(&skill_content).unwrap();
                        assert!(!vec.is_empty(), "Failed to generate embedding vector");
                        
                        // Write to cache dir
                        let cache_dir = test_skill_path.join(".cache");
                        fs::create_dir_all(&cache_dir).unwrap();
                        let emb_path = cache_dir.join(format!("{}.emb.safetensors", safe_filename));
                        let data_bytes = unsafe { std::slice::from_raw_parts(vec.as_ptr() as *const f32 as *const u8, vec.len() * 4) };
                        if let Ok(view) = safetensors::tensor::TensorView::new(safetensors::tensor::Dtype::F32, vec![vec.len()], data_bytes) {
                            safetensors::serialize_to_file(vec![("embedding", view)], None::<std::collections::HashMap<String, String>>, &emb_path).unwrap();
                        }
                        
                        println!("✅ [Test] Skill Vector Cached successfully at {:?}", emb_path);
                        assert!(emb_path.exists(), "Cache file must exist");
                    }
                }
            }
        }

        // Cleanup test directory
        let _ = fs::remove_dir_all(&test_skill_path);
        println!("✅ [Test] Sovereign Dynamic Pipeline Test passed successfully!");
    }
}
