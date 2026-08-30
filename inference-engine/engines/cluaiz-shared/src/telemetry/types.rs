use serde::{Deserialize, Serialize};

/// Telemetry metrics, hardware resource usage and logs for a single tool/skill
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ComponentTelemetryItem {
    pub name: String,
    pub category: String, // "skill" | "plugin" | "mcp"
    pub status: String,   // "deferred" | "active" | "reverted"
    pub security_mode: String, // "full_access" | "sandboxed" | "strict"
    pub tokens: usize,
    pub execution_latency_ms: f64,
    pub memory_used_mb: f64,
    pub memory_cap_mb: f64,
    pub cpu_fuel_consumed: u64,
    pub input_payload: Option<serde_json::Value>,
    pub output_result: Option<serde_json::Value>,
    pub logs: Vec<String>,
}

/// Category group with total count, tokens, percentage, and nested tree items
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CategoryTelemetryGroup {
    pub count: usize,
    pub tokens: usize,
    pub percent: f64,
    pub items: Vec<ComponentTelemetryItem>,
}

/// Real-time Context Window and Hardware Memory Telemetry Snapshot
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SystemContextTelemetry {
    /// Total configured context window limit (e.g., 32768)
    pub total_context_limit: usize,
    /// Current active token count (n_past)
    pub active_context_pos: usize,
    /// Tokens consumed by chat messages / history
    pub messages_tokens: usize,
    /// Tokens consumed by system prompt
    pub system_prompt_tokens: usize,
    /// Tokens consumed by active SKILL.md prompts
    pub active_skills_tokens: usize,
    /// Tokens consumed by active WASM/C-FFI plugin schemas
    pub active_plugins_tokens: usize,
    /// Tokens consumed by active MCP tool schemas
    pub active_mcp_tokens: usize,
    /// Free remaining token headroom
    pub free_space_tokens: usize,
    /// SAVED TOKENS (Tokens of all idle/installed components that were NOT dumped into context)
    pub deferred_saved_tokens: usize,
    /// Count of idle/deferred tools
    pub deferred_tools_count: usize,
    /// List of currently active tool / skill names
    pub active_tool_names: Vec<String>,
    /// Nested Tree Groups for Interactive Inspector
    pub skills: CategoryTelemetryGroup,
    pub plugins: CategoryTelemetryGroup,
    pub mcp_tools: CategoryTelemetryGroup,
    /// Base model weights VRAM in MB
    pub vram_weights_mb: u64,
    /// Dynamic KV-Cache VRAM in MB
    pub vram_kv_cache_mb: u64,
    /// Total allocated GPU VRAM in MB
    pub vram_total_mb: u64,
    /// Engine process working set RSS RAM in MB
    pub process_ram_mb: u64,
}
