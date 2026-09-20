use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::tools::registry::SecurityMode;
use super::environment::EnvironmentResolver;
use super::policy::{ExecutionPolicy, PolicyDecision};
use super::result::TerminalRunResult;

/// Stdin piping mode for child processes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StdinMode {
    #[default]
    Null,
    Piped,
    Inherit,
}

/// Handle allowing live cancellation of a running terminal command
#[derive(Clone)]
pub struct CancelHandle {
    sender: Arc<watch::Sender<bool>>,
}

impl CancelHandle {
    pub fn new() -> (Self, watch::Receiver<bool>) {
        let (sender, receiver) = watch::channel(false);
        (
            Self {
                sender: Arc::new(sender),
            },
            receiver,
        )
    }

    /// Signals the running command to terminate immediately
    pub fn cancel(&self) {
        let _ = self.sender.send(true);
    }
}

/// Manages sandboxed OS process execution.
/// Acts strictly as an Execution Backend without command heuristics or guessing.
pub struct SandboxTerminalRunner;

impl SandboxTerminalRunner {
    /// Validates a command against the active SecurityMode policy
    pub fn validate_command(command: &str, cwd: &Path, security_mode: SecurityMode) -> Result<()> {
        match ExecutionPolicy::evaluate(command, cwd, security_mode) {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::AskConfirmation { reason, command } => {
                Err(anyhow!("Confirmation required for command '{}': {}", command, reason))
            }
            PolicyDecision::Deny { reason } => {
                Err(anyhow!("Security policy violation: {}", reason))
            }
        }
    }

    /// Basic execution shorthand
    pub async fn execute(
        command_line: &str,
        cwd: &Path,
        security_mode: SecurityMode,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<TerminalRunResult> {
        Self::execute_advanced(command_line, cwd, security_mode, None, None, None, cancel_rx).await
    }

    /// Advanced execution with resolved environment, configurable stdin, timeout, and cancellation
    pub async fn execute_advanced(
        command_line: &str,
        cwd: &Path,
        security_mode: SecurityMode,
        venv_dir: Option<&Path>,
        stdin_input: Option<&str>,
        timeout_secs: Option<u64>,
        mut cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<TerminalRunResult> {
        Self::validate_command(command_line, cwd, security_mode)?;

        let start_time = Instant::now();
        let trimmed = command_line.trim();

        // 1. Resolve Environment & Credentials
        let scrub_secrets = matches!(security_mode, SecurityMode::Sandboxed | SecurityMode::Strict);
        let env_resolved = EnvironmentResolver::resolve(cwd, venv_dir, scrub_secrets);

        // 2. Select Platform Shell (Clean, Standard, Zero Keyword Sniffing)
        #[cfg(target_os = "windows")]
        let (shell_cmd, shell_args) = {
            if trimmed.starts_with("cmd.exe") || trimmed.starts_with("cmd /c") {
                ("cmd.exe", vec!["/c", trimmed])
            } else {
                ("powershell.exe", vec!["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", trimmed])
            }
        };

        #[cfg(not(target_os = "windows"))]
        let (shell_cmd, shell_args) = ("sh", vec!["-c", trimmed]);

        let stdin_mode = if stdin_input.is_some() {
            StdinMode::Piped
        } else {
            StdinMode::Null
        };

        // 3. Build Process Command
        let mut cmd = Command::new(shell_cmd);
        cmd.args(&shell_args)
            .current_dir(&env_resolved.cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Configure Stdin
        match stdin_mode {
            StdinMode::Null => { cmd.stdin(Stdio::null()); }
            StdinMode::Piped => { cmd.stdin(Stdio::piped()); }
            StdinMode::Inherit => { cmd.stdin(Stdio::inherit()); }
        }

        // Apply clean resolved environment
        cmd.env_clear();
        for (k, v) in &env_resolved.env_vars {
            cmd.env(k, v);
        }

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn sandbox process: {}", e))?;

        // 4. Handle Stdin Stream (with automatic pipe drop to prevent process deadlock)
        if let Some(mut stdin_pipe) = child.stdin.take() {
            if let Some(input_str) = stdin_input {
                let input_bytes = input_str.as_bytes().to_vec();
                tokio::spawn(async move {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin_pipe.write_all(&input_bytes).await;
                    let _ = stdin_pipe.flush().await;
                    drop(stdin_pipe);
                });
            } else {
                drop(stdin_pipe);
            }
        }

        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to open stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to open stderr"))?;

        // 5. Read stdout and stderr concurrently
        let stdout_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(line);
            }
            lines.join("\n")
        });

        let stderr_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(line);
            }
            lines.join("\n")
        });

        // 6. Execution Loop with Timeout & Cancellation
        let mut cancelled = false;
        let effective_timeout = Duration::from_secs(timeout_secs.unwrap_or(180));

        let status_res = tokio::select! {
            biased;
            _ = async {
                if let Some(ref mut rx) = cancel_rx {
                    while rx.changed().await.is_ok() {
                        if *rx.borrow() {
                            return;
                        }
                    }
                }
                std::future::pending::<()>().await;
            } => {
                tracing::warn!("🛑 [TerminalRunner] Command cancelled by user. Terminating process...");
                let _ = child.kill().await;
                cancelled = true;
                child.wait().await
            }
            _ = tokio::time::sleep(effective_timeout) => {
                tracing::warn!("⏱️ [TerminalRunner] Command exceeded timeout of {}s. Terminating...", effective_timeout.as_secs());
                let _ = child.kill().await;
                child.wait().await
            }
            status = child.wait() => {
                status
            }
        };

        let exit_code = match status_res {
            Ok(st) => st.code().unwrap_or(if cancelled { -1 } else { 1 }),
            Err(_) => if cancelled { -1 } else { 1 },
        };

        let stdout_raw = stdout_handle.await.unwrap_or_default();
        let stderr_raw = stderr_handle.await.unwrap_or_default();
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // 7. Format Normalized Result
        Ok(TerminalRunResult::new(
            exit_code,
            stdout_raw,
            stderr_raw,
            duration_ms,
            cancelled,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_runner_basic_echo() {
        let temp_dir = std::env::temp_dir();
        let (_cancel_handle, cancel_rx) = CancelHandle::new();

        let cmd = "echo 'Hello Cluaiz'";

        let result = SandboxTerminalRunner::execute(cmd, &temp_dir, SecurityMode::Sandboxed, Some(cancel_rx))
            .await
            .expect("Command execution should succeed");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Hello Cluaiz") || (result.stdout.contains("Hello") && result.stdout.contains("Cluaiz")));
        assert!(!result.cancelled);
    }

    #[test]
    fn test_dangerous_command_blocked() {
        let temp_dir = std::env::temp_dir();
        let res = SandboxTerminalRunner::validate_command("rm -rf /", &temp_dir, SecurityMode::Sandboxed);
        assert!(res.is_err(), "Destructive command must be blocked by validation");
    }

    #[tokio::test]
    async fn test_cancel_handle() {
        let temp_dir = std::env::temp_dir();
        let (cancel_handle, cancel_rx) = CancelHandle::new();

        #[cfg(target_os = "windows")]
        let cmd = "ping -n 10 127.0.0.1";
        #[cfg(not(target_os = "windows"))]
        let cmd = "sleep 10";

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            cancel_handle.cancel();
        });

        let result = SandboxTerminalRunner::execute(cmd, &temp_dir, SecurityMode::Sandboxed, Some(cancel_rx))
            .await
            .expect("Execution with cancel should handle gracefully");

        assert!(result.cancelled, "Command should have been cancelled by cancel handle");
    }

    #[tokio::test]
    async fn test_clean_shell_and_stdin() {
        let temp_dir = std::env::temp_dir();

        #[cfg(target_os = "windows")]
        let cmd = "$input | ForEach-Object { 'OUTPUT:' + $_ }";
        #[cfg(not(target_os = "windows"))]
        let cmd = "cat";

        let result = SandboxTerminalRunner::execute_advanced(
            cmd,
            &temp_dir,
            SecurityMode::Sandboxed,
            None,
            Some("Live Clean Stdin\n"),
            Some(30),
            None
        )
        .await
        .expect("Advanced execution with stdin should succeed");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Live Clean Stdin"), "Stdout must receive and process stdin");
    }
}
