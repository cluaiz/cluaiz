use crate::telemetry::types::SystemContextTelemetry;
use std::collections::HashMap;

/// Fast token estimator for rough sizing (approx 3.8 chars per token)
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    // Standard rule of thumb: max(1, chars / 3.8 rounded up)
    let chars = text.chars().count();
    ((chars as f64) / 3.8).ceil() as usize
}

/// Computes the exact mathematical KV-Cache VRAM consumption in MB
/// Formula: 2 * n_layers * n_heads * head_dim * bytes_per_element * n_ctx / (1024 * 1024)
pub fn calculate_kv_cache_vram_mb(
    n_layers: usize,
    n_heads: usize,
    head_dim: usize,
    bytes_per_element: usize,
    current_tokens: usize,
) -> u64 {
    let bytes = 2 * n_layers * n_heads * head_dim * bytes_per_element * current_tokens;
    (bytes / (1024 * 1024)) as u64
}

/// Token & Resource Accounting Engine
pub struct ContextTracker;

impl ContextTracker {
    /// Builds a full SystemContextTelemetry snapshot from active and idle components
    pub fn build_snapshot(
        total_limit: usize,
        messages_text: &str,
        system_prompt: &str,
        active_skills_content: &[String],
        active_plugins_schemas: &[String],
        active_mcp_schemas: &[String],
        idle_component_sizes: &HashMap<String, usize>,
        vram_weights_mb: u64,
        n_layers: usize,
        n_heads: usize,
        head_dim: usize,
        bytes_per_element: usize,
        process_ram_mb: u64,
    ) -> SystemContextTelemetry {
        let messages_tokens = estimate_tokens(messages_text);
        let system_prompt_tokens = estimate_tokens(system_prompt);

        let active_skills_tokens: usize = active_skills_content
            .iter()
            .map(|s| estimate_tokens(s))
            .sum();

        let active_plugins_tokens: usize = active_plugins_schemas
            .iter()
            .map(|s| estimate_tokens(s))
            .sum();

        let active_mcp_tokens: usize = active_mcp_schemas
            .iter()
            .map(|s| estimate_tokens(s))
            .sum();

        let total_used = messages_tokens
            + system_prompt_tokens
            + active_skills_tokens
            + active_plugins_tokens
            + active_mcp_tokens;

        let free_space = total_limit.saturating_sub(total_used);

        // Calculate tokens saved by NOT loading idle components upfront
        let deferred_saved_tokens: usize = idle_component_sizes.values().sum();
        let deferred_tools_count = idle_component_sizes.len();

        let vram_kv_cache_mb = calculate_kv_cache_vram_mb(
            n_layers,
            n_heads,
            head_dim,
            bytes_per_element,
            total_used,
        );

        SystemContextTelemetry {
            total_context_limit: total_limit,
            active_context_pos: total_used,
            messages_tokens,
            system_prompt_tokens,
            active_skills_tokens,
            active_plugins_tokens,
            active_mcp_tokens,
            free_space_tokens: free_space,
            deferred_saved_tokens,
            deferred_tools_count,
            active_tool_names: Vec::new(),
            vram_weights_mb,
            vram_kv_cache_mb,
            vram_total_mb: vram_weights_mb + vram_kv_cache_mb,
            process_ram_mb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        let text = "Hello world! This is a test.";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[test]
    fn test_kv_cache_vram_calculation() {
        // 32 layers, 32 heads, 128 dim, FP16 (2 bytes), 4096 tokens
        let mb = calculate_kv_cache_vram_mb(32, 32, 128, 2, 4096);
        assert_eq!(mb, 2048); // Exactly 2GB
    }
}
