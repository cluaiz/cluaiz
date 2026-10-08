use serde::{Deserialize, Serialize};

/// Maximum bytes kept inline before head-and-tail truncation (50KB standard)
pub const DEFAULT_MAX_STDOUT_BYTES: usize = 50 * 1024;
pub const DEFAULT_MAX_STDERR_BYTES: usize = 20 * 1024;

/// Execution output from a sandbox terminal command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_info: Option<String>,
}

impl TerminalRunResult {
    /// Creates a new TerminalRunResult with output truncation and signal explanation
    pub fn new(
        exit_code: i32,
        stdout_raw: String,
        stderr_raw: String,
        duration_ms: u64,
        cancelled: bool,
    ) -> Self {
        let stdout = Self::truncate_output(&stdout_raw, DEFAULT_MAX_STDOUT_BYTES);
        let stderr = Self::truncate_output(&stderr_raw, DEFAULT_MAX_STDERR_BYTES);
        let signal_info = Self::explain_exit_code(exit_code, cancelled);

        Self {
            exit_code,
            stdout,
            stderr,
            duration_ms,
            cancelled,
            signal_info,
        }
    }

    /// Truncates large output with head-and-tail preservation to avoid memory/context bloat
    pub fn truncate_output(text: &str, max_bytes: usize) -> String {
        if text.len() <= max_bytes {
            return text.to_string();
        }

        let half = max_bytes / 2;
        let head_idx = Self::floor_char_boundary(text, half);
        let tail_start = text.len().saturating_sub(half);
        let tail_idx = Self::ceil_char_boundary(text, tail_start);

        let head = &text[..head_idx];
        let tail = &text[tail_idx..];
        let omitted = text.len() - (head.len() + tail.len());

        format!(
            "{}\n\n[... output truncated: {} bytes omitted to prevent context flooding ...]\n\n{}",
            head, omitted, tail
        )
    }

    /// Explains exit codes and signals in human-readable terms (Hermes standard)
    pub fn explain_exit_code(exit_code: i32, cancelled: bool) -> Option<String> {
        if cancelled {
            return Some("Command was terminated by active user cancellation".to_string());
        }

        match exit_code {
            0 => None,
            -9 | 137 => Some("Terminated by SIGKILL (Signal 9) — commonly triggered by the host kernel OOM killer on memory exhaustion".to_string()),
            -15 | 143 => Some("Terminated by SIGTERM (Signal 15) — process was requested to terminate gracefully".to_string()),
            -11 | 139 => Some("Terminated by SIGSEGV (Signal 11) — segmentation fault: invalid memory address access".to_string()),
            -6 | 134 => Some("Terminated by SIGABRT (Signal 6) — abort signal raised".to_string()),
            127 => Some("Command not found (exit code 127) — binary or script does not exist on PATH".to_string()),
            126 => Some("Permission denied (exit code 126) — command found but not executable".to_string()),
            _ => None,
        }
    }

    fn floor_char_boundary(s: &str, index: usize) -> usize {
        if index >= s.len() {
            s.len()
        } else {
            let mut lower = index;
            while !s.is_char_boundary(lower) && lower > 0 {
                lower -= 1;
            }
            lower
        }
    }

    fn ceil_char_boundary(s: &str, index: usize) -> usize {
        if index >= s.len() {
            s.len()
        } else {
            let mut upper = index;
            while !s.is_char_boundary(upper) && upper < s.len() {
                upper += 1;
            }
            upper
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_small_output() {
        let small = "hello world";
        let out = TerminalRunResult::truncate_output(small, 100);
        assert_eq!(out, small);
    }

    #[test]
    fn test_truncate_large_output() {
        let large = "A".repeat(1000);
        let out = TerminalRunResult::truncate_output(&large, 200);
        assert!(out.contains("[... output truncated:"));
        assert!(out.len() < 1000);
    }

    #[test]
    fn test_signal_explanation() {
        let exp = TerminalRunResult::explain_exit_code(137, false);
        assert!(exp.is_some());
        assert!(exp.unwrap().contains("OOM killer"));

        let exp_cancel = TerminalRunResult::explain_exit_code(-1, true);
        assert!(exp_cancel.is_some());
        assert!(exp_cancel.unwrap().contains("cancellation"));
    }
}
