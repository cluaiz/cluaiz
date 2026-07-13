use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub architecture: String,
    pub parameters: String,
    pub bit_depth: String,
    pub context_window: String, // e.g., "256k", "8k"
    pub ram_required_gb: f64,
    pub family: String,
    pub category: String,
}

impl ModelManifest {
    /// Resolves the context window string into a numeric token count.
    /// Supports 'k' suffix for thousands.
    pub fn resolve_ctx(&self) -> usize {
        let clean = self.context_window.to_lowercase();
        if clean.contains('k') {
            let num_str: String = clean.chars().filter(|c| c.is_ascii_digit()).collect();
            num_str.parse::<usize>().unwrap_or(2048) * 1024
        } else {
            clean.parse::<usize>().unwrap_or(2048)
        }
    }
}
