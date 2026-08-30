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

/// Security mode for tool execution
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecurityMode {
    #[default]
    FullAccess,
    Sandboxed,
    Strict,
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

    /// Semantic version (optional, read dynamically from package.json)
    #[serde(default = "default_version", skip_serializing_if = "String::is_empty")]
    pub version: String,

    /// Short description of capabilities
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,

    /// Absolute or relative local directory on filesystem
    #[serde(default)]
    pub local_dir: String,

    /// Path to compiled WASM or native binary (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_path: Option<String>,

    /// Master ON/OFF toggle
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Security mode: "full_access" | "sandboxed" | "strict"
    #[serde(default)]
    pub security_mode: SecurityMode,

    /// Execution mode: "auto" (model calls tool) vs "manual" (user confirmation required)
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Default turn duration (-1 = permanent / auto-quiescence)
    #[serde(default = "default_persistent_turns", skip_serializing_if = "is_default_turn")]
    pub default_turns: i32,

    /// Granular permissions e.g. ["net:fetch", "fs:read", "cpu:fuel"]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,

    /// Words and phrases that activate this tool in context
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_triggers: Vec<String>,

    /// Explicit event strings (e.g. "on_command:use plugin::cluaiz-search")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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

fn is_default_turn(t: &i32) -> bool {
    *t == -1
}
