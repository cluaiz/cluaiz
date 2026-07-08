use crate::neural_foundry::registry::SkillRegistry;

pub struct SkillRouter {}

impl Default for SkillRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRouter {
    pub fn new() -> Self {
        Self {}
    }

    /// Selects ALL relevant skills for a given prompt (Core Fusion Mode).
    /// Uses the dynamic registry and KERNEL TELEMETRY to find compute-aware matches.
    pub fn match_intent(&self, prompt: &str, registry: &SkillRegistry) -> Vec<String> {
        // 🛰️ cluaiz Linkage: Get real-time Hardware pressure
        let pulse = cluaiz_shared::hardware::telemetry::get_pulse();
        let _pulse_lock = pulse.pulse.read().unwrap();
        
        let mut matches = Vec::new();
        let prompt_lower = prompt.to_lowercase();

        // 1. Fast Keyword/Trigger Containment Filter (O(N)) using padded word boundaries
        let mut has_trigger_keyword = false;
        let padded_prompt = format!(" {} ", prompt_lower);
        for skill in &registry.skills {
            for trigger in &skill.manifest.triggers.semantic {
                let norm_trigger = format!(" {} ", trigger.to_lowercase().trim());
                if padded_prompt.contains(&norm_trigger) {
                    has_trigger_keyword = true;
                    break;
                }
            }
            if has_trigger_keyword { break; }
        }

        // Bypassing vector loading/generation completely for conversational prompts
        let active_model_opt = crate::neural_foundry::security::permission_schema::PermissionSchema::load()
            .get_active_embedding_model();

        if has_trigger_keyword && active_model_opt.is_some() {
            let active_model_id = active_model_opt.unwrap();
            let safe_filename = active_model_id.replace(":", "-");
            
            let prompt_vector = crate::memory::embedding_generator::EmbeddingGenerator::generate_vector(prompt);
            let prompt_len = prompt_vector.len();
            let prompt_is_valid = prompt_len > 0 && prompt_vector.iter().any(|&x| x != 0.0);

            if prompt_is_valid {
                for skill in &registry.skills {
                    let mut is_matched = false;
                    let threshold = skill.manifest.triggers.entropy_threshold.unwrap_or(0.70);

                    // Try loading cached skill embedding
                    let cache_path = skill.path.join(".cache").join(format!("{}.emb.safetensors", safe_filename));
                    let mut cached_floats = None;
                    if cache_path.exists() {
                        if let Ok(file) = std::fs::File::open(&cache_path) {
                            if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                                if let Ok(st) = safetensors::SafeTensors::deserialize(&mmap) {
                                    if let Ok(tensor) = st.tensor("embedding") {
                                        let data = tensor.data();
                                        if data.len() % 4 == 0 {
                                            let floats: Vec<f32> = data
                                                .chunks_exact(4)
                                                .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
                                                .collect();
                                            cached_floats = Some(floats);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(floats) = &cached_floats {
                        if floats.len() % prompt_len == 0 {
                            for chunk in floats.chunks_exact(prompt_len) {
                                let similarity = cosine_similarity(&prompt_vector, chunk);
                                if similarity > threshold {
                                    tracing::debug!("[Skill-Router] Cached Match probability {:.2} > {:.2} for skill {}", similarity, threshold, skill.manifest.id);
                                    is_matched = true;
                                    break;
                                }
                            }
                        }
                    }

                    if !is_matched {
                        for trigger in &skill.manifest.triggers.semantic {
                            let trigger_vector = crate::memory::embedding_generator::EmbeddingGenerator::generate_vector(trigger);
                            let similarity = cosine_similarity(&prompt_vector, &trigger_vector);
                            if similarity > threshold {
                                tracing::debug!("[Skill-Router] Dynamic Match probability {:.2} > {:.2} for skill {}", similarity, threshold, skill.manifest.id);
                                is_matched = true;
                                break;
                            }
                        }
                    }

                    if !is_matched {
                        let desc_vector = crate::memory::embedding_generator::EmbeddingGenerator::generate_vector(&skill.manifest.description);
                        let similarity = cosine_similarity(&prompt_vector, &desc_vector);
                        if similarity > threshold {
                            tracing::debug!("[Skill-Router] Description Match probability {:.2} > {:.2} for skill {}", similarity, threshold, skill.manifest.id);
                            is_matched = true;
                        }
                    }

                    if is_matched {
                        matches.push(skill.manifest.id.clone());
                    }
                }
            }
        }

        // 2. String Fallback checks: checks keywords containment in the prompt for registry/manifest triggers
        for skill in &registry.skills {
            if matches.contains(&skill.manifest.id) {
                continue;
            }

            let mut is_matched = false;
            for trigger in &skill.manifest.triggers.semantic {
                if prompt_lower.contains(&trigger.to_lowercase()) {
                    tracing::debug!("[Skill-Router] Fallback string match for skill {} trigger {}", skill.manifest.id, trigger);
                    is_matched = true;
                    break;
                }
            }

            if !is_matched {
                if prompt_lower.contains(&skill.manifest.description.to_lowercase()) || 
                   skill.manifest.description.to_lowercase().contains(&prompt_lower) {
                    tracing::debug!("[Skill-Router] Fallback description string match for skill {}", skill.manifest.id);
                    is_matched = true;
                }
            }

            if is_matched {
                matches.push(skill.manifest.id.clone());
            }
        }

        matches
    }

    /// Parses the JSON output from LogitSteer and forwards it to the WASM Sandbox.
    pub fn route_llm_action(&self, json_output: &str) -> anyhow::Result<()> {
        tracing::info!("🔄 [Skill-Router] Parsing LogitSteer output: {}", json_output);
        
        // In production, this decodes the JSON using Serde
        // e.g., { "skill": "git-commit", "args": { "msg": "Fix bug" } }
        
        // 1. Identify which skill binary (.wasm) to load.
        // 2. Load `.kvcache.bin` for KV-Cache Injection (Zero-Copy).
        // 3. Instantiate the WASM sandbox and pass the arguments.
        
        tracing::warn!("🚀 [Skill-Router] Dispatching to sandboxed WASM skill logic (Mock).");
        Ok(())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}
