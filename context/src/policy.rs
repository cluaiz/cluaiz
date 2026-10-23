//! Planner policy: the knobs a request or the settings can change

use serde::{Deserialize, Serialize};

/// How the planner shares the context. Percentages are of `available` (see `planner`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// Newest whole turns always kept in full (when they fit at all)
    pub recent_turns_min: usize,
    /// Cap for this turn's recalled evidence
    pub evidence_cap_pct: u8,
    /// Cap for tool suggestions (cards and titles)
    pub suggestions_cap_pct: u8,
    /// Kept free for template tokens and tokenizer drift, as a share of `n_ctx`
    pub margin_pct: u8,
    /// A plan that uses this share of `available` or more asks for compaction
    pub compact_at_pct: u8,
}

impl Default for Policy {
    fn default() -> Self {
        Self { recent_turns_min: 2, evidence_cap_pct: 15, suggestions_cap_pct: 5, margin_pct: 2, compact_at_pct: 85 }
    }
}
