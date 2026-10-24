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
    /// A compaction frees enough to bring the history down to this share, so the next one is
    /// many turns away (each compaction re-processes the prompt from its first change)
    pub compact_to_pct: u8,
    /// Idle-time card enrichment by the model: "auto" (with GPU layers), "always", "rules_only",
    /// "off"
    pub indexer: String,
    /// Notes the model proposes with `remember`: "ask" (proposed until the user confirms),
    /// "auto" (confirmed at once) or "off" (no remember function)
    pub notes: String,
    /// Searching other chats with `recall` (scope workspace or all): "ask" (the user approves
    /// each search, like a tool call), "allow" or "deny"
    pub cross_chat: String,
    /// How tools reach the model: "inline" (schemas in the system prompt, calls as `<tool_call>`
    /// blocks) or "native" (the model's own template and call format, with llama.cpp's grammar
    /// and parser)
    pub tool_format: String,
    /// "on": when the suggestions name one clear tool, the Engine loads it before the model
    /// answers (one model step fewer); "off": the model loads tools itself
    pub auto_load: String,
    /// "on": the MCP server of a clearly suggested tool starts in the background before the model
    /// asks for it (its first load does not wait for the start); "off": servers start when loaded
    pub mcp_prewarm: String,
}

impl Default for Policy {
    fn default() -> Self {
        Self { recent_turns_min: 2, evidence_cap_pct: 15, suggestions_cap_pct: 5, margin_pct: 2, compact_at_pct: 85, compact_to_pct: 60, indexer: "auto".into(), notes: "ask".into(), cross_chat: "ask".into(), tool_format: "native".into(), auto_load: "off".into(), mcp_prewarm: "on".into() }
    }
}
