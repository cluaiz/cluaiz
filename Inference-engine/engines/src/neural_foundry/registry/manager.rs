use super::SkillRegistry;

impl SkillRegistry {
    /// 🚀 Cluaize Pull: Downloads and installs a skill from the Global Hub.
    pub async fn install_skill(skill_name: &str) -> anyhow::Result<()> {
        use colored::Colorize;
        cluaize_shared::dev_info!("\n  {} [Cluaize] Contacting Universal Skill Registry...", "📡".cyan());
        
        let skills_dir = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_skills_dir()
            .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().skills_dir())
            .join(skill_name);
        
        cluaize_shared::dev_info!("  {} [Cluaize] Installing skill '{}'...", "🚀".green(), skill_name.bold());

        let registry_url = "https://raw.githubusercontent.com/cluaiz/skills/main/registry.json";
        let client = reqwest::Client::new();
        let registry_resp = client.get(registry_url).send().await;
        
        let mut download_url = String::new();
        
        if let Ok(resp) = registry_resp {
            if resp.status().is_success() {
                if let Ok(registry_json) = resp.json::<serde_json::Value>().await {
                    if let Some(skills_obj) = registry_json.get("skills").and_then(|s| s.as_object()) {
                        if let Some(skill_data) = skills_obj.get(skill_name) {
                            if let Some(latest) = skill_data.get("latest").and_then(|v| v.as_str()) {
                                if let Some(versions) = skill_data.get("versions").and_then(|v| v.as_object()) {
                                    if let Some(url) = versions.get(latest).and_then(|u| u.as_str()) {
                                        download_url = url.to_string();
                                        cluaize_shared::dev_info!("  {} [Registry] Found skill release: v{}", "✅".green(), latest.bold());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if download_url.is_empty() {
            cluaize_shared::dev_info!("  {} [Registry] Skill '{}' not found or has no valid release in the registry.", "❌".red(), skill_name.bold());
            let skills_dir_clone = skills_dir.clone();
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_dir(&skills_dir_clone);
            }).await?;
            return Err(anyhow::anyhow!("Skill not found in registry"));
        }

        cluaize_shared::dev_info!("  {} [Cluaize] Downloading release package...", "⬇️".cyan());
        let zip_resp = client.get(&download_url).send().await?;
        
        if !zip_resp.status().is_success() {
            let skills_dir_clone = skills_dir.clone();
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_dir(&skills_dir_clone);
            }).await?;
            return Err(anyhow::anyhow!("Failed to download skill package"));
        }
        
        let zip_bytes = zip_resp.bytes().await?;
        
        let skill_name_string = skill_name.to_string();
        let skills_dir_clone = skills_dir.clone();

        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            if !skills_dir_clone.exists() {
                std::fs::create_dir_all(&skills_dir_clone)?;
            }

            let temp_zip_path = skills_dir_clone.join(format!("{}.zip", skill_name_string));
            
            let mut file = std::fs::File::create(&temp_zip_path)?;
            file.write_all(&zip_bytes)?;
            
            cluaize_shared::dev_info!("  {} [Cluaize] Extracting package...", "📦".cyan());
            let status = std::process::Command::new("tar")
                .arg("-xf")
                .arg(&temp_zip_path)
                .arg("-C")
                .arg(&skills_dir_clone)
                .status()?;
                
            if !status.success() {
                let _ = std::fs::remove_file(&temp_zip_path);
                let _ = std::fs::remove_dir(&skills_dir_clone);
                return Err(anyhow::anyhow!("Extraction failed"));
            }
            
            let _ = std::fs::remove_file(&temp_zip_path);

            cluaize_shared::dev_info!("\n  {} [Cluaize] Skill '{}' successfully installed and registered!\n", "✅".green(), skill_name_string.bold());

            let manifest_path = skills_dir_clone.join("manifest.json");
            let parsed_manifest = if manifest_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    serde_json::from_str::<crate::neural_foundry::registry::SkillManifest>(&content).ok()
                } else {
                    None
                }
            } else {
                let skill_md_path = skills_dir_clone.join("SKILL.md");
                if skill_md_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&skill_md_path) {
                        if let Some(start) = content.find("---\n") {
                            if let Some(end) = content[start + 4..].find("\n---") {
                                let yaml_content = &content[start + 4..start + 4 + end];
                                serde_yaml::from_str::<crate::neural_foundry::registry::SkillManifest>(yaml_content).ok()
                            } else { None }
                        } else { None }
                    } else { None }
                } else {
                    None
                }
            };

            if let Some(mut manifest) = parsed_manifest {
                if manifest.id.is_empty() {
                    manifest.id = manifest.name.clone();
                }
                
                let permissions = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
                if let Some(embedding_model_id) = permissions.get_active_embedding_model() {
                    let roster = crate::models::registry::CoreRoster::load_roster();
                    if let Some(model_manifest) = roster.iter().find(|m| m.id == embedding_model_id) {
                        if let Some(local_path) = &model_manifest.local_path {
                            let model_dir = std::path::Path::new(local_path);
                            let model_file = model_dir.join("model.onnx");
                            let tokenizer_file = model_dir.join("tokenizer.json");
                            if model_file.exists() && tokenizer_file.exists() {
                                cluaize_shared::dev_info!("  {} [Cluaize] Compiling skill vector immediately...", "⚙️".cyan());
                                let cache_dir = skills_dir_clone.join(".cache");
                                let _ = std::fs::create_dir_all(&cache_dir);
                                let safe_filename = embedding_model_id.replace(":", "-");
                                let embedding_cache_path = cache_dir.join(format!("{}.emb.bin", safe_filename));
                                
                                let skill_content = if let Some(fm) = Self::extract_frontmatter(&skills_dir_clone) {
                                    fm
                                } else {
                                    let semantic_triggers = manifest.triggers.semantic.join(", ");
                                    format!(
                                        "Skill Name: {}\nDescription: {}\nTriggers: {}",
                                        manifest.name, manifest.description, semantic_triggers
                                    )
                                };

                                if let Ok(mut engine) = cluaize_onnx::engine::OnnxEngine::new() {
                                    if engine.load_text_model(&model_file.to_string_lossy(), &tokenizer_file.to_string_lossy()).is_ok() {
                                        if let Ok(vec) = neural_core::interfaces::router_contract::EmbeddingDriver::gen_embedding(&mut engine, &skill_content) {
                                            let data_bytes = unsafe { std::slice::from_raw_parts(vec.as_ptr() as *const f32 as *const u8, vec.len() * 4) };
                                            if let Err(e) = std::fs::write(&embedding_cache_path, data_bytes) {
                                                cluaize_shared::dev_info!("❌ Failed to write binary embedding: {}", e);
                                            } else {
                                                cluaize_shared::dev_info!("✅ Real Router Embedding generated: {:?}", embedding_cache_path);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        }).await?;
        Ok(())
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

    pub fn list_installed_skills() -> anyhow::Result<Vec<String>> {
        let skills_dir = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_skills_dir()
            .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().skills_dir());
        let mut skills = Vec::new();

        if skills_dir.exists() {
            for entry in std::fs::read_dir(skills_dir)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        skills.push(name.to_string());
                    }
                }
            }
        }
        Ok(skills)
    }

    pub fn list_skills_cache() -> anyhow::Result<String> {
        let skills_dir = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_skills_dir()
            .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().skills_dir());
        
        if !skills_dir.exists() {
            return Ok("No skills installed.".to_string());
        }
        
        let mut total_size = 0;
        let mut cache_count = 0;
        let mut report = String::new();
        
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let cache_dir = path.join(".cache");
                if cache_dir.exists() {
                    for cache_entry in std::fs::read_dir(&cache_dir)? {
                        let cache_entry = cache_entry?;
                        let cache_path = cache_entry.path();
                        if cache_path.is_file() {
                            if let Ok(meta) = cache_entry.metadata() {
                                total_size += meta.len();
                                cache_count += 1;
                                let name = cache_path.file_name().unwrap_or_default().to_string_lossy();
                                let size_mb = meta.len() as f64 / 1_048_576.0;
                                report.push_str(&format!("ðŸ”¹ {} | Size: {:.2} MB\n", name, size_mb));
                            }
                        }
                    }
                }
            }
        }
        report.push_str(&format!("\nTotal Caches: {} | Total Size: {:.2} MB", cache_count, total_size as f64 / 1_048_576.0));
        Ok(report)
    }

    pub fn clear_skills_cache(model_id: Option<String>, all: bool, force: bool) -> anyhow::Result<usize> {
        let skills_dir = cluaize_shared::environment::EnvironmentManager::current()
            .ensure_skills_dir()
            .unwrap_or_else(|_| cluaize_shared::environment::EnvironmentManager::current().skills_dir());
        if !skills_dir.exists() { return Ok(0); }
        
        let mut wiped = 0;
        for entry in std::fs::read_dir(&skills_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let cache_dir = path.join(".cache");
                if cache_dir.exists() {
                    for cache_entry in std::fs::read_dir(&cache_dir)? {
                        let cache_entry = cache_entry?;
                        let cache_path = cache_entry.path();
                        if cache_path.is_file() {
                            let name = cache_path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            let mut should_delete = false;
                            
                            if all {
                                should_delete = force;
                            } else if let Some(target_id) = &model_id {
                                if name == *target_id {
                                    should_delete = true;
                                }
                            }
                            
                            if should_delete {
                                if std::fs::remove_file(&cache_path).is_ok() {
                                    wiped += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(wiped)
    }
}
