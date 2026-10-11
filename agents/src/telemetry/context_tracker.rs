use super::types::{CategoryTelemetryGroup, ComponentTelemetryItem, ContextBreakdown, SystemContextTelemetry};
use crate::registry::ToolsRegistry;

pub struct ContextTracker;

impl ContextTracker {
    /// Computes full context telemetry for a given model, session, actual prompt/history/system lengths, active tools, and generated completion tokens
    pub fn compute_telemetry(
        active_model_id: &str,
        session_id: &str,
        active_tool_ids: &[String],
        user_prompt_len: usize,
        history_len: usize,
        system_prompt_len: usize,
        generated_tokens: usize,
    ) -> SystemContextTelemetry {
        let registry = ToolsRegistry::load().unwrap_or_default();
        let perm = cluaiz_security::PermissionSchema::load();
        
        // Native limit: the model's own context length from the registry (0 = unknown)
        let slot_model = perm.active_slots.get("chat_slot").and_then(|s| s.model_id.clone()).unwrap_or_default();
        let model_key = if !active_model_id.is_empty() && active_model_id != "default" { active_model_id.to_string() } else { slot_model };
        let model_native_limit = cluaiz_models::registry::InstalledStateRegistry::load()
            .installed_models
            .get(&model_key)
            .map(|m| cluaiz_models::StructuralDNA::parse_context_string(&m.metadata.context_window))
            .unwrap_or(0);

        // Usable limit: the context llama.cpp created for the loaded model; 0 until a model is loaded
        let usable_limit = cluaiz_core::active_context_tokens().map(|n| n as usize).unwrap_or(0);

        let mut skills_tokens = 0;
        let mut plugins_tokens = 0;
        let mut mcp_tools_tokens = 0;
        let mut deferred_mcp_tokens = 0;
        let mut deferred_plugins_tokens = 0;
        let mut deferred_saved = 0;

        let mut skill_items = Vec::new();
        let mut plugin_items = Vec::new();
        let mut mcp_items = Vec::new();

        for (id, entry) in &registry.installed_tools {
            if !entry.enabled {
                continue;
            }

            let is_active = active_tool_ids.contains(id);

            // Real schema / instruction token estimation
            let estimated_tokens = if entry.category == "skill" {
                let skill_file = std::path::Path::new(&entry.local_dir).join("SKILL.md");
                let skill_path = if skill_file.exists() {
                    Some(skill_file)
                } else {
                    let env_skill = cluaiz_core::EnvironmentManager::current()
                        .skills_dir()
                        .join(id)
                        .join("SKILL.md");
                    if env_skill.exists() {
                        Some(env_skill)
                    } else {
                        None
                    }
                };

                if let Some(path) = skill_path {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Some(parsed) = crate::skills::SkillParser::parse_content(&content) {
                            ((parsed.prompt_instructions.len() + 3) / 4).max(4)
                        } else {
                            ((content.len() + 3) / 4).max(4)
                        }
                    } else {
                        let schema_chars = entry.description.len() 
                            + entry.permissions.iter().map(|s| s.len()).sum::<usize>()
                            + entry.semantic_triggers.iter().map(|s| s.len()).sum::<usize>();
                        if schema_chars > 0 { (schema_chars / 4).max(4) } else { 4 }
                    }
                } else {
                    let schema_chars = entry.description.len() 
                        + entry.permissions.iter().map(|s| s.len()).sum::<usize>()
                        + entry.semantic_triggers.iter().map(|s| s.len()).sum::<usize>();
                    if schema_chars > 0 { (schema_chars / 4).max(4) } else { 4 }
                }
            } else {
                let schema_chars = entry.description.len() 
                    + entry.permissions.iter().map(|s| s.len()).sum::<usize>()
                    + entry.semantic_triggers.iter().map(|s| s.len()).sum::<usize>();
                if schema_chars > 0 {
                    (schema_chars / 4).max(4)
                } else {
                    4
                }
            };

            let item = ComponentTelemetryItem {
                name: if entry.name.is_empty() { id.clone() } else { entry.name.clone() },
                category: entry.category.clone(),
                status: if is_active { "active".to_string() } else { "deferred".to_string() },
                security_mode: format!("{:?}", entry.security_mode).to_lowercase(),
                tokens: if is_active { estimated_tokens } else { 0 },
                execution_latency_ms: 0.0,
                memory_used_mb: 0.0,
                memory_cap_mb: 0.0,
                cpu_fuel_consumed: 0,
                input_payload: None,
                output_result: None,
                logs: Vec::new(),
            };

            if is_active {
                match entry.category.as_str() {
                    "skill" => {
                        skills_tokens += estimated_tokens;
                        skill_items.push(item);
                    }
                    "plugin" => {
                        plugins_tokens += estimated_tokens;
                        plugin_items.push(item);
                    }
                    "mcp" => {
                        mcp_tools_tokens += estimated_tokens;
                        mcp_items.push(item);
                    }
                    _ => {
                        plugins_tokens += estimated_tokens;
                        plugin_items.push(item);
                    }
                }
            } else {
                deferred_saved += estimated_tokens;
                match entry.category.as_str() {
                    "mcp" => {
                        deferred_mcp_tokens += estimated_tokens;
                    }
                    "skill" => {}
                    _ => {
                        deferred_plugins_tokens += estimated_tokens;
                    }
                }
            }
        }

        // Account for any active tool IDs not present in registry.installed_tools
        for active_id in active_tool_ids {
            let clean_active_id = active_id.trim();
            if !clean_active_id.is_empty() && !skill_items.iter().any(|item| item.name == clean_active_id) {
                let env = cluaiz_core::EnvironmentManager::current();
                let skill_dir = env.skills_dir().join(clean_active_id);
                let skill_file = skill_dir.join("SKILL.md");
                if skill_file.exists() {
                    let est_tokens = if let Ok(content) = std::fs::read_to_string(&skill_file) {
                        if let Some(parsed) = crate::skills::SkillParser::parse_content(&content) {
                            ((parsed.prompt_instructions.len() + 3) / 4).max(4)
                        } else {
                            ((content.len() + 3) / 4).max(4)
                        }
                    } else {
                        0
                    };
                    skills_tokens += est_tokens;
                    skill_items.push(ComponentTelemetryItem {
                        name: clean_active_id.to_string(),
                        category: "skill".to_string(),
                        status: "active".to_string(),
                        security_mode: "sandboxed".to_string(),
                        tokens: est_tokens,
                        execution_latency_ms: 0.0,
                        memory_used_mb: 0.0,
                        memory_cap_mb: 0.0,
                        cpu_fuel_consumed: 0,
                        input_payload: None,
                        output_result: None,
                        logs: Vec::new(),
                    });
                }
            }
        }

        // Real token calculation from character lengths (1 token ~= 4 chars)
        let system_prompt_tokens = if system_prompt_len > 0 {
            (system_prompt_len / 4).max(1)
        } else {
            0
        };
        let user_prompt_tokens = if user_prompt_len > 0 {
            (user_prompt_len / 4).max(1)
        } else {
            0
        };
        let chat_history_tokens = if history_len > 0 {
            history_len / 4
        } else {
            0
        };
        
        let messages_tokens = user_prompt_tokens + chat_history_tokens + generated_tokens;
        let active_tools_tokens = skills_tokens + plugins_tokens + mcp_tools_tokens;
        
        let total_active = (system_prompt_tokens + messages_tokens + active_tools_tokens).min(usable_limit);
        let free_space = usable_limit.saturating_sub(total_active);
        
        let pct = |t: usize| -> f64 {
            if usable_limit > 0 {
                let val = ((t as f64) / (usable_limit as f64) * 1000.0).round() / 10.0;
                val.clamp(0.0, 100.0)
            } else {
                0.0
            }
        };

        let skills_group = CategoryTelemetryGroup {
            count: skill_items.len(),
            tokens: skills_tokens,
            percent: pct(skills_tokens),
            items: skill_items,
        };

        let plugins_group = CategoryTelemetryGroup {
            count: plugin_items.len(),
            tokens: plugins_tokens,
            percent: pct(plugins_tokens),
            items: plugin_items,
        };

        let mcp_group = CategoryTelemetryGroup {
            count: mcp_items.len(),
            tokens: mcp_tools_tokens,
            percent: pct(mcp_tools_tokens),
            items: mcp_items,
        };

        let kv_cache_vram_bytes = total_active * 2 * 32 * 128;

        SystemContextTelemetry {
            context_breakdown: ContextBreakdown {
                total_context_limit: usable_limit,
                model_native_context: model_native_limit,
                total_active_tokens: total_active,
                active_percentage: pct(total_active),
                messages_tokens: messages_tokens.min(usable_limit),
                messages_percentage: pct(messages_tokens),
                system_prompt_tokens,
                system_prompt_percentage: pct(system_prompt_tokens),
                skills_tokens,
                skills_percentage: pct(skills_tokens),
                plugins_tokens,
                plugins_percentage: pct(plugins_tokens),
                mcp_tools_tokens,
                mcp_tools_percentage: pct(mcp_tools_tokens),
                free_space_tokens: free_space,
                free_space_percentage: pct(free_space),
                skills: skills_group,
                plugins: plugins_group,
                mcp_tools: mcp_group,
                deferred_mcp_tokens,
                deferred_plugins_tokens,
                deferred_tools_tokens_saved: deferred_saved,
                system_tools_tokens: plugins_tokens,
                system_tools_percentage: pct(plugins_tokens),
                deferred_system_tools_tokens: deferred_plugins_tokens,
                base_system_tokens: system_prompt_tokens,
                active_tools_tokens,
                active_tools_count: active_tool_ids.len(),
                user_prompt_tokens,
                chat_history_tokens,
            },
            kv_cache_vram_bytes_allocated: kv_cache_vram_bytes,
            active_session_id: session_id.to_string(),
        }
    }
}
