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
    Sandboxed,
    FullAccess,
    WorkspaceWrite,
    Strict,
}

impl SecurityMode {
    pub fn from_raw(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "sandboxed" => Some(Self::Sandboxed),
            "workspace_write" | "workspacewrite" | "workspace-write" => Some(Self::WorkspaceWrite),
            "strict" => Some(Self::Strict),
            "full_access" | "fullaccess" | "full-access" => Some(Self::FullAccess),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sandboxed => "sandboxed",
            Self::FullAccess => "full_access",
            Self::WorkspaceWrite => "workspace_write",
            Self::Strict => "strict",
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

    /// Declared capabilities e.g. ["exec", "fs_write", "network", "fs_read"]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
}

impl ToolEntry {
    /// Evaluates if this tool execution requires Human-In-The-Loop approval
    /// based on master security mode, category, execution mode, and declared capabilities.
    pub fn requires_approval(&self, master_security_mode: &str) -> bool {
        match master_security_mode.to_lowercase().as_str() {
            "full_access" => false,
            "strict" => true,
            _ => {
                // In Sandboxed mode:
                // 1. External MCP tools always require approval (privilege boundary)
                if self.category.to_lowercase() == "mcp" {
                    return true;
                }
                // 2. Explicit tool-level strict mode
                if self.security_mode == SecurityMode::Strict {
                    return true;
                }
                // 3. Execution mode requires confirmation
                if self.execution_mode == ExecutionMode::Manual
                    || self.execution_mode == ExecutionMode::ConfirmDestructive
                {
                    return true;
                }
                // 4. Capability inspection
                let caps = if !self.capabilities.is_empty() {
                    &self.capabilities
                } else {
                    &self.permissions
                };
                // Directive 1: Undeclared tools = approval by default
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
        assert_eq!(SecurityMode::from_raw("sandboxed"), Some(SecurityMode::Sandboxed));
        assert_eq!(SecurityMode::from_raw("workspace_write"), Some(SecurityMode::WorkspaceWrite));
        assert_eq!(SecurityMode::from_raw("workspace-write"), Some(SecurityMode::WorkspaceWrite));
        assert_eq!(SecurityMode::from_raw("strict"), Some(SecurityMode::Strict));
        assert_eq!(SecurityMode::from_raw("full_access"), Some(SecurityMode::FullAccess));
        assert_eq!(SecurityMode::from_raw("full-access"), Some(SecurityMode::FullAccess));
        assert_eq!(SecurityMode::from_raw("unknown"), None);
    }

    #[test]
    fn test_serde_roundtrip() {
        let entry_json = r#"{
            "id": "test-tool",
            "name": "Test Tool",
            "security_mode": "workspace_write",
            "execution_mode": "confirm_destructive"
        }"#;
        let parsed: ToolEntry = serde_json::from_str(entry_json).unwrap();
        assert_eq!(parsed.security_mode, SecurityMode::WorkspaceWrite);
        assert_eq!(parsed.execution_mode, ExecutionMode::ConfirmDestructive);

        let serialized = serde_json::to_string(&parsed).unwrap();
        assert!(serialized.contains(r#""security_mode":"workspace_write""#));
        assert!(serialized.contains(r#""execution_mode":"confirm_destructive""#));
    }

    #[test]
    fn test_requires_approval_capabilities() {
        // 1. Full access always bypasses
        let mut tool = ToolEntry::default();
        tool.capabilities = vec!["exec".to_string()];
        assert!(!tool.requires_approval("full_access"));

        // 2. Strict always prompts
        tool.capabilities = vec!["fs_read".to_string()];
        assert!(tool.requires_approval("strict"));

        // 3. Sandboxed: MCP always prompts
        let mut mcp_tool = ToolEntry::default();
        mcp_tool.category = "mcp".to_string();
        assert!(mcp_tool.requires_approval("sandboxed"));

        // 4. Sandboxed: Undeclared capabilities -> default to approval
        let undeclared_tool = ToolEntry::default();
        assert!(undeclared_tool.requires_approval("sandboxed"));

        // 5. Sandboxed: Dangerous capability -> prompts
        let mut exec_tool = ToolEntry::default();
        exec_tool.capabilities = vec!["exec".to_string()];
        assert!(exec_tool.requires_approval("sandboxed"));

        let mut write_tool = ToolEntry::default();
        write_tool.capabilities = vec!["fs_write".to_string()];
        assert!(write_tool.requires_approval("sandboxed"));

        // 6. Sandboxed: Safe read-only tool -> allowed without approval
        let mut safe_tool = ToolEntry::default();
        safe_tool.capabilities = vec!["fs_read".to_string(), "read_only".to_string()];
        assert!(!safe_tool.requires_approval("sandboxed"));
    }
}
