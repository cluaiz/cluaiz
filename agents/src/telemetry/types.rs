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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SystemContextTelemetry {
    pub context_breakdown: ContextBreakdown,
    pub kv_cache_vram_bytes_allocated: usize,
    pub active_session_id: String,
}
