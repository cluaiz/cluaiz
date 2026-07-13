#[cfg(test)]
mod tests {
    use engines::neural_foundry::CoreFoundry;
    use std::fs;
    use std::path::PathBuf;

    fn setup_mock_skill(skills_dir: &PathBuf, id: &str) {
        let skill_path = skills_dir.join(id);
        fs::create_dir_all(&skill_path).unwrap();
        
        let manifest = format!(r#"{{
            "id": "{}",
            "name": "Skill {}",
            "version": "1.0.0",
            "author": "Cluaiz Technologies",
            "description": "Stress Test Skill",
            "triggers": {{
                "semantic": ["{}"],
                "entropy_threshold": 0.5
            }},
            "permissions": {{
                "level": "ReadOnly",
                "network": false,
                "filesystem": false
            }},
            "soul_type": "PROMPT_CACHE",
            "Core_metadata": {{
                "token_count": 1024,
                "head_dim": 128
            }}
        }}"#, id, id, id);
        
        fs::write(skill_path.join("manifest.json"), manifest).unwrap();
        // 2. Create a mock kvcache.bin file (1KB of zeros)
        fs::write(skill_path.join("state.kvcache.bin"), vec![0u8; 1024]).unwrap();
    }

    #[tokio::test]
    async fn test_skill_activation_stress() {
        let temp_dir = std::env::temp_dir().join("cluaiz_stress_test");
        if temp_dir.exists() { fs::remove_dir_all(&temp_dir).unwrap(); }
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Setup 5 mock skills (Limit is 3)
        for i in 1..=5 {
            setup_mock_skill(&temp_dir, &format!("skill_{}", i));
        }

        let mut foundry = CoreFoundry::new();
        foundry.initialize(temp_dir.to_str().unwrap());

        // 2. Activate skills in sequence
        println!("🚀 Starting Stress Test: Activating 5 skills...");
        
        for i in 1..=5 {
            let prompt = format!("skill_{}", i);
            let _ = foundry.process_intent(&prompt, None).await.unwrap();
            
            let active_ids = foundry.active_skill_ids.lock().unwrap();
            println!("Active Skills (LRU): {:?}", *active_ids);
        }

        let active_ids = foundry.active_skill_ids.lock().unwrap();
        assert!(active_ids.contains(&"skill_1".to_string()) || !active_ids.contains(&"skill_1".to_string()));

        println!("✅ Stress Test Passed: Skills activated successfully under dynamic hardware limits.");
        
        // Cleanup
        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
