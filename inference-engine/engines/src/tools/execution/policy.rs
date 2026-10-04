use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::tools::registry::SecurityMode;

use super::policy_rules::{HARD_DENIED_PATTERNS, SENSITIVE_SYSTEM_PATHS, READ_ONLY_COMMAND_PREFIXES};

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

        // 1. Prohibited Destructive Patterns (Always Denied across Windows, Linux, and macOS)
        for pattern in HARD_DENIED_PATTERNS {
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
            SecurityMode::Inherit => {
                // Cross-platform sensitive credentials, tokens, and system security paths
                for path in SENSITIVE_SYSTEM_PATHS {
                    if lower.contains(path) {
                        return PolicyDecision::Deny {
                            reason: format!("Inherited security mode prevents access to sensitive system path: '{}'", path),
                        };
                    }
                }

                // Warn on recursive delete in workspace
                if lower.starts_with("rm -rf")
                    || lower.starts_with("rmdir /s")
                    || lower.starts_with("rd /s")
                    || lower.starts_with("remove-item -recurse")
                {
                    return PolicyDecision::AskConfirmation {
                        reason: "Recursive directory deletion in workspace".to_string(),
                        command: trimmed.to_string(),
                    };
                }

                PolicyDecision::Allow
            }
            SecurityMode::RequireApproval => {
                // RequireApproval mode allows safe read-only inspection commands, requires prompt for mutations
                let is_read_only = READ_ONLY_COMMAND_PREFIXES.iter().any(|p| {
                    lower == *p
                        || lower.starts_with(&format!("{} ", p))
                        || lower.starts_with(&format!("{}.", p))
                        || lower.starts_with(&format!("{}/", p))
                        || lower.starts_with(&format!("{}\\", p))
                });

                if is_read_only {
                    PolicyDecision::Allow
                } else {
                    PolicyDecision::AskConfirmation {
                        reason: "Security policy requires user confirmation for execution".to_string(),
                        command: trimmed.to_string(),
                    }
                }
            }
            SecurityMode::AlwaysAllow => {
                PolicyDecision::Allow
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hard_denied_destructive_commands_all_os() {
        let cwd = std::env::temp_dir();

        // Linux / POSIX destructive commands
        assert!(matches!(ExecutionPolicy::evaluate("rm -rf /", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("mkfs.ext4 /dev/sda1", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("dd if=/dev/zero of=/dev/sda", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate(":(){ :|:& };:", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));

        // Windows destructive commands
        assert!(matches!(ExecutionPolicy::evaluate("format C: /Q", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("rmdir /s /q C:\\Windows", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("Clear-Disk -Number 0", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("Stop-Computer -Force", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));

        // macOS destructive commands
        assert!(matches!(ExecutionPolicy::evaluate("diskutil eraseDisk APFS Untitled /dev/disk2", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("csrutil disable", &cwd, SecurityMode::AlwaysAllow), PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_cross_platform_sensitive_paths_blocked_in_sandbox() {
        let cwd = std::env::temp_dir();

        // SSH & Cloud tokens
        assert!(matches!(ExecutionPolicy::evaluate("cat ~/.ssh/id_ed25519", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("cat ~/.aws/credentials", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));
        assert!(matches!(ExecutionPolicy::evaluate("cat ~/.kube/config", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));

        // Linux shadow hive
        assert!(matches!(ExecutionPolicy::evaluate("head /etc/shadow", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));

        // Windows SAM hive
        assert!(matches!(ExecutionPolicy::evaluate("type C:\\Windows\\System32\\config\\SAM", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));

        // macOS keychain
        assert!(matches!(ExecutionPolicy::evaluate("security dump-keychain ~/Library/Keychains/login.keychain", &cwd, SecurityMode::Inherit), PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_non_existent_cwd_denied() {
        let invalid_cwd = Path::new("Z:\\non_existent_path_xyz_123");
        let decision = ExecutionPolicy::evaluate("echo hello", invalid_cwd, SecurityMode::Inherit);
        assert!(matches!(decision, PolicyDecision::Deny { .. }));
    }

    #[test]
    fn test_strict_mode_confirmation_and_read_only_allowed() {
        let cwd = std::env::temp_dir();
        // Mutations require prompt
        let decision = ExecutionPolicy::evaluate("npm install axios", &cwd, SecurityMode::RequireApproval);
        assert!(matches!(decision, PolicyDecision::AskConfirmation { .. }));

        let del_decision = ExecutionPolicy::evaluate("del myfile.txt", &cwd, SecurityMode::RequireApproval);
        assert!(matches!(del_decision, PolicyDecision::AskConfirmation { .. }));

        // Read-only inspection commands are allowed without prompt across OS
        assert_eq!(ExecutionPolicy::evaluate("cargo check", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("git status --short", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("git diff HEAD~1", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("ls -la", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("dir /w", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("Get-ChildItem -Path .", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("npm test", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
        assert_eq!(ExecutionPolicy::evaluate("uname -a", &cwd, SecurityMode::RequireApproval), PolicyDecision::Allow);
    }

    #[test]
    fn test_sandboxed_safe_commands_allowed() {
        let cwd = std::env::temp_dir();
        let decision = ExecutionPolicy::evaluate("cargo test", &cwd, SecurityMode::Inherit);
        assert_eq!(decision, PolicyDecision::Allow);
    }
}
