use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ContextBreakdown {
    pub base_system_tokens: usize,
    pub active_tools_tokens: usize,
    pub active_tools_count: usize,
    pub user_prompt_tokens: usize,
    pub chat_history_tokens: usize,
    pub deferred_tools_tokens_saved: usize,
    pub total_active_tokens: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SystemContextTelemetry {
    pub context_breakdown: ContextBreakdown,
    pub kv_cache_vram_bytes_allocated: usize,
    pub active_session_id: String,
}
