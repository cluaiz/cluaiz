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
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Auto,
    Manual,
    ConfirmDestructive,
}

impl ExecutionMode {
    pub fn from_raw(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "manual" => Some(Self::Manual),
            "confirm_destructive" | "confirmdestructive" | "confirm-destructive" => Some(Self::ConfirmDestructive),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::ConfirmDestructive => "confirm_destructive",
        }
    }
}

/// Security mode for tool execution
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecurityMode {
    #[default]
    Inherit,
    RequireApproval,
    AlwaysAllow,
}

impl SecurityMode {
    pub fn from_raw(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "inherit" => Some(Self::Inherit),
            "require_approval" => Some(Self::RequireApproval),
            "always_allow" => Some(Self::AlwaysAllow),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::RequireApproval => "require_approval",
            Self::AlwaysAllow => "always_allow",
        }
    }
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

    /// Security mode: "full_access" | "sandboxed" | "strict"
    #[serde(default)]
    pub security_mode: SecurityMode,

    /// Execution mode: "auto" (model calls tool) vs "manual" (user confirmation required)
    #[serde(default)]
    pub execution_mode: ExecutionMode,

    /// Default turn duration (-1 = permanent / auto-quiescence)
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

    /// Declared capabilities e.g. ["exec", "fs_write", "network", "fs_read"]
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl ToolEntry {
    /// Evaluates if this tool execution requires Human-In-The-Loop approval
    /// based on master security mode, category, execution mode, and declared capabilities.
    pub fn requires_approval(&self, master_security_mode: &str) -> bool {
        match master_security_mode.to_lowercase().as_str() {
            "full_access" => {
                // In Full Access: tool executes automatically unless user specifically marked it RequireApproval
                self.security_mode == SecurityMode::RequireApproval
            }
            "strict" => {
                // In Strict: all executions require approval unless user explicitly gave AlwaysAllow
                self.security_mode != SecurityMode::AlwaysAllow
            }
            _ => {
                // Sandboxed mode:
                // 1. Tool-level explicit overrides
                if self.security_mode == SecurityMode::AlwaysAllow {
                    return false;
                }
                if self.security_mode == SecurityMode::RequireApproval {
                    return true;
                }

                // 2. Inherit mode:
                // External MCP tools always require approval (privilege boundary)
                if self.category.to_lowercase() == "mcp" {
                    return true;
                }
                // Execution mode requires confirmation
                if self.execution_mode == ExecutionMode::Manual
                    || self.execution_mode == ExecutionMode::ConfirmDestructive
                {
                    return true;
                }
                // Capability inspection
                let caps = if !self.capabilities.is_empty() {
                    &self.capabilities
                } else {
                    &self.permissions
                };
                // Undeclared tools = approval by default
                if caps.is_empty() {
                    return true;
                }
                // Sensitive capabilities require approval
                caps.iter().any(|c| {
                    let lc = c.to_lowercase();
                    lc.contains("exec")
                        || lc.contains("write")
                        || lc.contains("net")
                        || lc.contains("shell")
                        || lc.contains("cmd")
                        || lc.contains("terminal")
                })
            }
        }
    }

    /// Returns resolved capabilities (or fallback to permissions or ["undeclared"])
    pub fn effective_capabilities(&self) -> Vec<String> {
        if !self.capabilities.is_empty() {
            self.capabilities.clone()
        } else if !self.permissions.is_empty() {
            self.permissions.clone()
        } else {
            vec!["undeclared".to_string()]
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_mode_parsing() {
        assert_eq!(ExecutionMode::from_raw("auto"), Some(ExecutionMode::Auto));
        assert_eq!(ExecutionMode::from_raw("manual"), Some(ExecutionMode::Manual));
        assert_eq!(ExecutionMode::from_raw("confirm_destructive"), Some(ExecutionMode::ConfirmDestructive));
        assert_eq!(ExecutionMode::from_raw("confirm-destructive"), Some(ExecutionMode::ConfirmDestructive));
        assert_eq!(ExecutionMode::from_raw("invalid"), None);
    }

    #[test]
    fn test_security_mode_parsing() {
        assert_eq!(SecurityMode::from_raw("inherit"), Some(SecurityMode::Inherit));
        assert_eq!(SecurityMode::from_raw("require_approval"), Some(SecurityMode::RequireApproval));
        assert_eq!(SecurityMode::from_raw("always_allow"), Some(SecurityMode::AlwaysAllow));
        assert_eq!(SecurityMode::from_raw("unknown"), None);
    }

    #[test]
    fn test_serde_roundtrip() {
        let entry_json = r#"{
            "id": "test-tool",
            "name": "Test Tool",
            "security_mode": "require_approval",
            "execution_mode": "confirm_destructive"
        }"#;
        let parsed: ToolEntry = serde_json::from_str(entry_json).unwrap();
        assert_eq!(parsed.security_mode, SecurityMode::RequireApproval);
        assert_eq!(parsed.execution_mode, ExecutionMode::ConfirmDestructive);

        let serialized = serde_json::to_string(&parsed).unwrap();
        assert!(serialized.contains(r#""security_mode":"require_approval""#));
        assert!(serialized.contains(r#""execution_mode":"confirm_destructive""#));
    }

    #[test]
    fn test_matrix_9_combinations() {
        // Combination 1: full_access + always_allow -> false (auto-run)
        let mut tool1 = ToolEntry::default();
        tool1.security_mode = SecurityMode::AlwaysAllow;
        assert!(!tool1.requires_approval("full_access"));

        // Combination 2: full_access + inherit -> false (auto-run)
        let mut tool2 = ToolEntry::default();
        tool2.security_mode = SecurityMode::Inherit;
        assert!(!tool2.requires_approval("full_access"));

        // Combination 3: full_access + require_approval -> true (prompt)
        let mut tool3 = ToolEntry::default();
        tool3.security_mode = SecurityMode::RequireApproval;
        assert!(tool3.requires_approval("full_access"));

        // Combination 4: sandboxed + always_allow -> false (auto-run)
        let mut tool4 = ToolEntry::default();
        tool4.security_mode = SecurityMode::AlwaysAllow;
        assert!(!tool4.requires_approval("sandboxed"));

        // Combination 5: sandboxed + inherit -> capability check
        let mut tool5_safe = ToolEntry::default();
        tool5_safe.security_mode = SecurityMode::Inherit;
        tool5_safe.capabilities = vec!["fs_read".to_string(), "read_only".to_string()];
        assert!(!tool5_safe.requires_approval("sandboxed"));

        let mut tool5_danger = ToolEntry::default();
        tool5_danger.security_mode = SecurityMode::Inherit;
        tool5_danger.capabilities = vec!["exec".to_string()];
        assert!(tool5_danger.requires_approval("sandboxed"));

        // Combination 6: sandboxed + require_approval -> true (prompt)
        let mut tool6 = ToolEntry::default();
        tool6.security_mode = SecurityMode::RequireApproval;
        assert!(tool6.requires_approval("sandboxed"));

        // Combination 7: strict + always_allow -> false (auto-run for explicitly trusted tool)
        let mut tool7 = ToolEntry::default();
        tool7.security_mode = SecurityMode::AlwaysAllow;
        assert!(!tool7.requires_approval("strict"));

        // Combination 8: strict + inherit -> true (prompt)
        let mut tool8 = ToolEntry::default();
        tool8.security_mode = SecurityMode::Inherit;
        assert!(tool8.requires_approval("strict"));

        // Combination 9: strict + require_approval -> true (prompt)
        let mut tool9 = ToolEntry::default();
        tool9.security_mode = SecurityMode::RequireApproval;
        assert!(tool9.requires_approval("strict"));
    }
}
