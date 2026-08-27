use super::types::{ContextBreakdown, SystemContextTelemetry};
use crate::tools::registry::ToolsRegistry;

pub struct ContextTracker;

impl ContextTracker {
    /// Computes full context telemetry for a given session and active tools
    pub fn compute_telemetry(session_id: &str, active_tool_ids: &[String], user_prompt_len: usize, history_len: usize) -> SystemContextTelemetry {
        let registry = ToolsRegistry::load().unwrap_or_default();
        let mut active_tools_tokens = 0;
        let mut total_ecosystem_tokens = 0;

        for (id, entry) in &registry.installed_tools {
            let estimated_tokens = entry.description.len() / 4 + 50;
            total_ecosystem_tokens += estimated_tokens;
            if active_tool_ids.contains(id) {
                active_tools_tokens += estimated_tokens;
            }
        }

        let base_system_tokens = 256;
        let user_prompt_tokens = user_prompt_len / 4;
        let chat_history_tokens = history_len / 4;
        let deferred_saved = total_ecosystem_tokens.saturating_sub(active_tools_tokens);
        let total_active = base_system_tokens + active_tools_tokens + user_prompt_tokens + chat_history_tokens;

        // VRAM calculation: ~2 bytes per token per layer/head KV cache
        let kv_cache_vram_bytes = total_active * 2 * 32 * 128;

        SystemContextTelemetry {
            context_breakdown: ContextBreakdown {
                base_system_tokens,
                active_tools_tokens,
                active_tools_count: active_tool_ids.len(),
                user_prompt_tokens,
                chat_history_tokens,
                deferred_tools_tokens_saved: deferred_saved,
                total_active_tokens: total_active,
            },
            kv_cache_vram_bytes_allocated: kv_cache_vram_bytes,
            active_session_id: session_id.to_string(),
        }
    }
}
