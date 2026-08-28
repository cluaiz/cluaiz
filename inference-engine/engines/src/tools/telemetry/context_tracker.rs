use super::types::{ContextBreakdown, SystemContextTelemetry};
use crate::tools::registry::ToolsRegistry;

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
        let gguf_meta = cluaiz_shared::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();
        
        // 🎯 100% Dynamic Model Native Context from InstalledStateRegistry / model_registry.json
        let installed_reg = crate::models::InstalledStateRegistry::load();
        let mut model_native_limit = 0;

        let clean_target = active_model_id.trim().to_lowercase().replace(['-', '_', ' ', '.', ':'], "");
        let target_entry = installed_reg.installed_models.get(active_model_id)
            .or_else(|| {
                installed_reg.installed_models.values().find(|m| {
                    if clean_target.is_empty() {
                        return false;
                    }
                    let clean_id = m.id.to_lowercase().replace(['-', '_', ' ', '.', ':'], "");
                    clean_id == clean_target || clean_id.contains(&clean_target) || clean_target.contains(&clean_id)
                })
            })
            .or_else(|| {
                // Fallback to active chat model in permission schema
                let perm = crate::neural_foundry::security::permission_schema::PermissionSchema::load();
                let perm_id_opt: Option<String> = perm.active_slots.get("chat_slot").and_then(|s| s.model_id.clone())
                    .or(perm.chat_models.text);
                if let Some(pid) = perm_id_opt {
                    let clean_pid = pid.to_lowercase().replace(['-', '_', ' ', '.', ':'], "");
                    installed_reg.installed_models.get(&pid).or_else(|| {
                        installed_reg.installed_models.values().find(|m| {
                            let clean_id = m.id.to_lowercase().replace(['-', '_', ' ', '.', ':'], "");
                            clean_id == clean_pid || clean_id.contains(&clean_pid) || clean_pid.contains(&clean_id)
                        })
                    })
                } else {
                    None
                }
            })
            .or_else(|| installed_reg.installed_models.values().next());

        if let Some(model_entry) = target_entry {
            let ctx_str = &model_entry.metadata.context_window;
            if ctx_str.ends_with('k') || ctx_str.ends_with('K') {
                if let Ok(k_val) = ctx_str[..ctx_str.len() - 1].parse::<usize>() {
                    model_native_limit = k_val * 1024;
                }
            } else if let Ok(exact) = ctx_str.parse::<usize>() {
                model_native_limit = exact;
            }
        }

        if model_native_limit == 0 {
            model_native_limit = if gguf_meta.hardware_and_execution.n_ctx > 0 {
                gguf_meta.hardware_and_execution.n_ctx as usize
            } else {
                4096
            };
        }
        
        // Usable Context Limit (hardware safety clamped)
        let usable_limit = if gguf_meta.hardware_and_execution.n_ctx > 0 {
            gguf_meta.hardware_and_execution.n_ctx as usize
        } else {
            4096
        }.max(2048);

        let mut skills_tokens = 0;
        let mut plugins_tokens = 0;
        let mut mcp_tools_tokens = 0;
        
        let mut deferred_mcp_tokens = 0;
        let mut deferred_plugins_tokens = 0;
        let mut deferred_saved = 0;

        for (id, entry) in &registry.installed_tools {
            if !entry.enabled {
                continue;
            }

            // 🛡️ Physical Disk Check: Reject phantom/dummy tools whose directory or binary does not exist on disk!
            let dir_exists = std::path::Path::new(&entry.local_dir).exists();
            let bin_exists = entry.binary_path.as_ref().map(|p| std::path::Path::new(p).exists()).unwrap_or(false);
            if !dir_exists && !bin_exists {
                continue;
            }

            // Real schema token estimation from description + permissions + triggers
            let schema_chars = entry.description.len() 
                + entry.permissions.iter().map(|s| s.len()).sum::<usize>()
                + entry.semantic_triggers.iter().map(|s| s.len()).sum::<usize>();
            
            // Only count if tool has real schema parameters
            let estimated_tokens = if schema_chars > 0 {
                (schema_chars / 4).max(4)
            } else {
                0
            };

            if estimated_tokens == 0 {
                continue;
            }

            if active_tool_ids.contains(id) {
                match entry.category.as_str() {
                    "skill" => skills_tokens += estimated_tokens,
                    "plugin" => plugins_tokens += estimated_tokens,
                    "mcp" => mcp_tools_tokens += estimated_tokens,
                    _ => plugins_tokens += estimated_tokens,
                }
            } else {
                deferred_saved += estimated_tokens;
                match entry.category.as_str() {
                    "mcp" => deferred_mcp_tokens += estimated_tokens,
                    "skill" => {}, // Inactive skills take zero active prompt space
                    _ => deferred_plugins_tokens += estimated_tokens,
                }
            }
        }

        // Real token calculation from character lengths (1 token ~= 3.8 to 4 chars)
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
        
        // Total active conversation tokens includes input prompt, chat history, AND generated output tokens in KV cache
        let messages_tokens = user_prompt_tokens + chat_history_tokens + generated_tokens;
        let active_tools_tokens = skills_tokens + plugins_tokens + mcp_tools_tokens;
        
        let total_active = system_prompt_tokens + messages_tokens + active_tools_tokens;
        let free_space = usable_limit.saturating_sub(total_active);
        
        let pct = |t: usize| -> f64 {
            if usable_limit > 0 {
                ((t as f64) / (usable_limit as f64) * 1000.0).round() / 10.0
            } else {
                0.0
            }
        };

        let kv_cache_vram_bytes = total_active * 2 * 32 * 128;

        SystemContextTelemetry {
            context_breakdown: ContextBreakdown {
                total_context_limit: usable_limit,
                model_native_context: model_native_limit,
                total_active_tokens: total_active,
                active_percentage: pct(total_active),
                messages_tokens,
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
                deferred_mcp_tokens,
                deferred_plugins_tokens,
                deferred_tools_tokens_saved: deferred_saved,
                // Backward-compatibility aliases
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

