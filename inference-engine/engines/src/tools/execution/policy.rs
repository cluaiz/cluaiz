use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::tools::registry::SecurityMode;

/// Outcome of evaluating an execution request against security policies
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Command is safe and permitted to run automatically
    Allow,
    /// Command involves sensitive or potentially destructive actions requiring user confirmation
    AskConfirmation {
        reason: String,
        command: String,
    },
    /// Command is strictly prohibited and denied
    Deny {
        reason: String,
    },
}

/// Evaluates command safety, arguments, and working directory boundaries
pub struct ExecutionPolicy;

impl ExecutionPolicy {
    /// Evaluates a command against the active SecurityMode
    pub fn evaluate(command: &str, cwd: &Path, security_mode: SecurityMode) -> PolicyDecision {
        let trimmed = command.trim();
        let lower = trimmed.to_lowercase();

        // 1. Universal Prohibited Destructive Patterns (Always Denied)
        let universal_denied = [
            "rm -rf /",
            "rm -rf /*",
            "rm -rf ~",
            "mkfs",
            "dd if=",
            "format c:",
            "rmdir /s /q c:\\",
            "del /f /s /q c:\\windows",
            ":(){ :|:& };:",
            "shutdown /s",
            "shutdown -h",
            "init 0",
        ];

        for pattern in universal_denied {
            if lower.contains(pattern) {
                return PolicyDecision::Deny {
                    reason: format!("Destructive command pattern '{}' is strictly prohibited across all modes", pattern),
                };
            }
        }

        // 2. Working Directory Verification
        if !cwd.exists() {
            return PolicyDecision::Deny {
                reason: format!("Target working directory does not exist: {}", cwd.display()),
            };
        }

        // 3. SecurityMode Specific Enforcement
        match security_mode {
            SecurityMode::Sandboxed => {
                // Suspicious commands that access sensitive host areas
                let sensitive_paths = [
                    "/etc/shadow",
                    "/etc/passwd",
                    "c:\\windows\\system32\\config",
                    "\\windows\\system32",
                ];
                for path in sensitive_paths {
                    if lower.contains(path) {
                        return PolicyDecision::Deny {
                            reason: format!("Sandboxed mode prevents access to sensitive system path: '{}'", path),
                        };
                    }
                }

                // Warn on recursive delete or package uninstall in sandbox
                if lower.starts_with("rm -rf") || lower.starts_with("rmdir /s") {
                    return PolicyDecision::AskConfirmation {
                        reason: "Recursive directory deletion in sandboxed workspace".to_string(),
                        command: trimmed.to_string(),
                    };
                }

                PolicyDecision::Allow
            }
            SecurityMode::Strict => {
                // Strict mode requires explicit confirmation for anything beyond basic read operations
                let read_only_prefixes = [
                    "git status", "git log", "git diff", "cargo check", "cargo test",
                    "ls", "dir", "cat", "echo", "pwd", "type"
                ];

                let is_read_only = read_only_prefixes.iter().any(|p| lower.starts_with(p));
                if is_read_only {
                    PolicyDecision::Allow
                } else {
                    PolicyDecision::AskConfirmation {
                        reason: "Strict security mode requires user confirmation for execution".to_string(),
                        command: trimmed.to_string(),
                    }
                }
            }
            SecurityMode::WorkspaceWrite | SecurityMode::FullAccess => {
                PolicyDecision::Allow
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_destructive_commands_denied() {
        let cwd = std::env::temp_dir();

        let decision = ExecutionPolicy::evaluate("rm -rf /", &cwd, SecurityMode::FullAccess);
        assert!(matches!(decision, PolicyDecision::Deny { .. }));

        let decision2 = ExecutionPolicy::evaluate("format C: /Q", &cwd, SecurityMode::FullAccess);
        assert!(matches!(decision2, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_non_existent_cwd_denied() {
        let invalid_cwd = Path::new("Z:\\non_existent_path_xyz_123");
        let decision = ExecutionPolicy::evaluate("echo hello", invalid_cwd, SecurityMode::Sandboxed);
        assert!(matches!(decision, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_strict_mode_confirmation() {
        let cwd = std::env::temp_dir();
        let decision = ExecutionPolicy::evaluate("npm install axios", &cwd, SecurityMode::Strict);
        assert!(matches!(decision, PolicyDecision::AskConfirmation { .. }));

        let read_decision = ExecutionPolicy::evaluate("cargo check", &cwd, SecurityMode::Strict);
        assert_eq!(read_decision, PolicyDecision::Allow);
    }

    #[test]
    fn test_sandboxed_safe_commands_allowed() {
        let cwd = std::env::temp_dir();
        let decision = ExecutionPolicy::evaluate("cargo test", &cwd, SecurityMode::Sandboxed);
        assert_eq!(decision, PolicyDecision::Allow);
    }
}
