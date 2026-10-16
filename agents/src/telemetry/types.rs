use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ComponentTelemetryItem {
    pub name: String,
    pub category: String,
    pub status: String,
    pub security_mode: String,
    pub tokens: usize,
    pub execution_latency_ms: f64,
    pub memory_used_mb: f64,
    pub memory_cap_mb: f64,
    pub cpu_fuel_consumed: u64,
    pub input_payload: Option<serde_json::Value>,
    pub output_result: Option<serde_json::Value>,
    pub logs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CategoryTelemetryGroup {
    pub count: usize,
    pub tokens: usize,
    pub percent: f64,
    pub items: Vec<ComponentTelemetryItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ContextBreakdown {
    pub total_context_limit: usize,
    pub model_native_context: usize,
    pub total_active_tokens: usize,
    pub active_percentage: f64,
    
    // Categorized Active Tokens matching 3-Pillars & UI Popover
    pub messages_tokens: usize,
    pub messages_percentage: f64,
    
    pub system_prompt_tokens: usize,
    pub system_prompt_percentage: f64,
    
    pub skills_tokens: usize,
    pub skills_percentage: f64,
    
    pub plugins_tokens: usize,
    pub plugins_percentage: f64,
    
    pub mcp_tools_tokens: usize,
    pub mcp_tools_percentage: f64,
    
    pub free_space_tokens: usize,
    pub free_space_percentage: f64,

    // Tree Category Groups for Drill-Down Inspector
    pub skills: CategoryTelemetryGroup,
    pub plugins: CategoryTelemetryGroup,
    pub mcp_tools: CategoryTelemetryGroup,
    
    // Deferred (Saved Tokens)
    pub deferred_mcp_tokens: usize,
    pub deferred_plugins_tokens: usize,
    pub deferred_tools_tokens_saved: usize,
    
    // Backward-compatibility aliases
    pub system_tools_tokens: usize,
    pub system_tools_percentage: f64,
    pub deferred_system_tools_tokens: usize,
    pub base_system_tokens: usize,
    pub active_tools_tokens: usize,
    pub active_tools_count: usize,
    pub user_prompt_tokens: usize,
    pub chat_history_tokens: usize,
}

impl ContextBreakdown {
    /// Replaces the character estimates with the engine's measured context: `n_ctx` as llama.cpp
    /// created it and `n_tokens` held after the generation (prompt after context shifting + answer).
    /// System prompt and tool shares keep their estimates inside that total; messages get the rest,
    /// so the shares and the free space add up to the context.
    pub fn apply_measured(&mut self, n_ctx: usize, n_tokens: usize) {
        let pct = |t: usize| -> f64 {
            if n_ctx > 0 {
                (((t as f64) / (n_ctx as f64) * 1000.0).round() / 10.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        };
        let total = n_tokens.min(n_ctx);
        let fixed = (self.system_prompt_tokens
            + self.skills_tokens
            + self.plugins_tokens
            + self.mcp_tools_tokens)
            .min(total);
        self.total_context_limit = n_ctx;
        self.total_active_tokens = total;
        self.active_percentage = pct(total);
        self.messages_tokens = total - fixed;
        self.messages_percentage = pct(self.messages_tokens);
        self.system_prompt_percentage = pct(self.system_prompt_tokens);
        self.skills_percentage = pct(self.skills_tokens);
        self.plugins_percentage = pct(self.plugins_tokens);
        self.mcp_tools_percentage = pct(self.mcp_tools_tokens);
        self.skills.percent = self.skills_percentage;
        self.plugins.percent = self.plugins_percentage;
        self.mcp_tools.percent = self.mcp_tools_percentage;
        self.free_space_tokens = n_ctx - total;
        self.free_space_percentage = pct(self.free_space_tokens);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SystemContextTelemetry {
    pub context_breakdown: ContextBreakdown,
    pub kv_cache_vram_bytes_allocated: usize,
    pub active_session_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_context_shares_add_up() {
        let mut b = ContextBreakdown {
            system_prompt_tokens: 100,
            skills_tokens: 50,
            messages_tokens: 9000,
            ..Default::default()
        };
        b.apply_measured(4096, 3000);
        assert_eq!(b.total_context_limit, 4096);
        assert_eq!(b.total_active_tokens, 3000);
        assert_eq!(b.messages_tokens, 2850);
        assert_eq!(b.free_space_tokens, 1096);
        assert!(b.messages_percentage <= 100.0);
        b.apply_measured(4096, 9999);
        assert_eq!((b.total_active_tokens, b.free_space_tokens), (4096, 0));
    }
}
