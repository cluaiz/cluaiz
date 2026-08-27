use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LoadStrategy {
    Eager,
    #[default]
    Lazy,
}

/// Execution mode for installed tools and skills
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    #[default]
    Auto,
    Manual,
}

/// Tool category in the Cluaiz ecosystem
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolCategory {
    Skill,
    Plugin,
    Mcp,
}

impl Default for ToolCategory {
    fn default() -> Self {
        Self::Plugin
    }
}

/// A single standardized entry in `tools_registry.json`
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ToolEntry {
    /// Unique tool identifier (e.g. "cluaiz-search", "frontend-dev")
    #[serde(default)]
    pub id: String,

    /// Human readable display name
    #[serde(default)]
    pub name: String,

    /// Category: "skill", "plugin", or "mcp"
    #[serde(default)]
    pub category: String,

    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,

    /// Short description of capabilities
    #[serde(default)]
    pub description: String,

    /// Absolute or relative local directory on filesystem
    #[serde(default)]
    pub local_dir: String,

    /// Path to compiled WASM or native binary (if applicable)
    #[serde(default)]
    pub binary_path: Option<String>,

    /// Master ON/OFF toggle
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Execution mode: "auto" (model calls tool) vs "manual" (user confirmation required)
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Default turn duration:
    /// -1 = Permanent (All-time active in session)
    ///  0 = Ephemeral (1-turn, auto-unloaded after response)
    ///  N = N-turns countdown (decrements each turn)
    #[serde(default = "default_persistent_turns")]
    pub default_turns: i32,

    /// Granular permissions e.g. ["net:fetch", "fs:read", "cpu:fuel"]
    #[serde(default)]
    pub permissions: Vec<String>,

    /// Words and phrases that activate this tool in context
    #[serde(default)]
    pub semantic_triggers: Vec<String>,

    /// Explicit event strings (e.g. "on_command:use plugin::cluaiz-search")
    #[serde(default)]
    pub activation_events: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_persistent_turns() -> i32 {
    -1
}
