use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{anyhow, Result};
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

    /// Direct process execution with exact argument vector boundary preservation (bypasses shell expansion)
    pub async fn execute_direct(
        program: &str,
        args: &[String],
        cwd: &Path,
        security_mode: SecurityMode,
    ) -> Result<TerminalRunResult> {
        Self::validate_command(program, cwd, security_mode)?;

        let start_time = Instant::now();
        let scrub_secrets = matches!(security_mode, SecurityMode::Inherit | SecurityMode::RequireApproval);
        let env_resolved = EnvironmentResolver::resolve(cwd, None, scrub_secrets);

        let (stdout, stdout_writer) = std::io::pipe()?;
        let (stderr, stderr_writer) = std::io::pipe()?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&env_resolved.cwd)
            .stdout(stdout_writer)
            .stderr(stderr_writer)
            .stdin(Stdio::null());

        cmd.env_clear();
        for (k, v) in &env_resolved.env_vars {
            cmd.env(k, v);
        }

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn process '{}': {}", program, e))?;

        // The command holds the writers; the reads end only when they are all closed
        drop(cmd);
        let stdout_handle = read_output(stdout, None);
        let stderr_handle = read_output(stderr, None);

        let status = child.wait().await?;
        let duration_ms = start_time.elapsed().as_millis() as u64;
        let stdout_str = stdout_handle.await.unwrap_or_default();
        let stderr_str = stderr_handle.await.unwrap_or_default();

        Ok(TerminalRunResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: stdout_str,
            stderr: stderr_str,
            duration_ms,
            cancelled: false,
            signal_info: None,
        })
    }

    /// Advanced execution with resolved environment, configurable stdin, timeout, and cancellation
    pub async fn execute_advanced(
        command_line: &str,
        cwd: &Path,
        security_mode: SecurityMode,
        venv_dir: Option<&Path>,
        stdin_input: Option<&str>,
        timeout_secs: Option<u64>,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<TerminalRunResult> {
        Self::execute_streaming(command_line, cwd, security_mode, venv_dir, stdin_input, timeout_secs, cancel_rx, None).await
    }

    /// [`Self::execute_advanced`] that also sends each stdout / stderr line to `on_line` as it is
    /// written, for a live view of a long command. The result still holds the whole output.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_streaming(
        command_line: &str,
        cwd: &Path,
        security_mode: SecurityMode,
        venv_dir: Option<&Path>,
        stdin_input: Option<&str>,
        timeout_secs: Option<u64>,
        mut cancel_rx: Option<watch::Receiver<bool>>,
        on_line: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    ) -> Result<TerminalRunResult> {
        Self::validate_command(command_line, cwd, security_mode)?;

        let start_time = Instant::now();
        let trimmed = command_line.trim();

        // 1. Resolve Environment & Credentials
        let scrub_secrets = matches!(security_mode, SecurityMode::Inherit | SecurityMode::RequireApproval);
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
        let (stdout, stdout_writer) = std::io::pipe()?;
        let (stderr, stderr_writer) = std::io::pipe()?;
        let mut cmd = Command::new(shell_cmd);
        cmd.args(&shell_args)
            .current_dir(&env_resolved.cwd)
            .stdout(stdout_writer)
            .stderr(stderr_writer);

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

        // 5. Read stdout and stderr concurrently. The command holds the writers; the reads end
        // only when they are all closed
        drop(cmd);
        let stdout_handle = read_output(stdout, on_line.clone());
        let stderr_handle = read_output(stderr, on_line);

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

/// Reads a command's output on its own thread, line by line; bytes that are not UTF-8 (a console
/// code page) become replacement characters instead of ending the read. Not tokio's child pipes:
/// on Windows a read of those ended the whole program ("operation failed to complete synchronously").
fn read_output(
    pipe: std::io::PipeReader,
    live: Option<tokio::sync::mpsc::UnboundedSender<String>>,
) -> tokio::task::JoinHandle<String> {
    tokio::task::spawn_blocking(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut lines = Vec::new();
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&bytes).trim_end_matches(['\r', '\n']).to_owned();
            if let Some(live) = &live {
                let _ = live.send(line.clone());
            }
            lines.push(line);
        }
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn output_that_is_not_utf8_is_kept() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"one\r\ntw\xFFo\nlast").unwrap();
        drop(writer);
        assert_eq!(read_output(reader, None).await.unwrap(), "one\ntw\u{FFFD}o\nlast");
    }

    /// Many commands at once. A pipe read the wrong way ended the whole program here on Windows.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore = "takes minutes"]
    async fn many_commands_at_once_do_not_end_the_program() {
        let dir = std::env::temp_dir();
        for round in 0..30 {
            let mut runs = Vec::new();
            for _ in 0..16 {
                let dir = dir.clone();
                runs.push(tokio::spawn(async move {
                    SandboxTerminalRunner::execute("Start-Sleep -Milliseconds 400; echo out", &dir, SecurityMode::Inherit, None).await
                }));
            }
            for run in runs {
                let result = run.await.unwrap().unwrap();
                assert_eq!(result.exit_code, 0);
                assert!(result.stdout.contains("out"), "round {round}: {result:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_sandbox_runner_basic_echo() {
        let temp_dir = std::env::temp_dir();
        let (_cancel_handle, cancel_rx) = CancelHandle::new();

        let cmd = "echo 'Hello Cluaiz'";

        let result = SandboxTerminalRunner::execute(cmd, &temp_dir, SecurityMode::Inherit, Some(cancel_rx))
            .await
            .expect("Command execution should succeed");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Hello Cluaiz") || (result.stdout.contains("Hello") && result.stdout.contains("Cluaiz")));
        assert!(!result.cancelled);
    }

    #[tokio::test]
    async fn test_streaming_sends_each_line_while_keeping_the_output() {
        let temp_dir = std::env::temp_dir();
        #[cfg(target_os = "windows")]
        let cmd = "Write-Output one; Write-Output two";
        #[cfg(not(target_os = "windows"))]
        let cmd = "echo one; echo two";
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let result = SandboxTerminalRunner::execute_streaming(cmd, &temp_dir, SecurityMode::Inherit, None, None, Some(30), None, Some(tx))
            .await
            .expect("streaming run");
        let mut live = Vec::new();
        while let Ok(line) = rx.try_recv() {
            live.push(line);
        }
        assert_eq!(live, ["one", "two"]);
        assert!(result.stdout.contains("one") && result.stdout.contains("two"));
    }

    #[test]
    fn test_dangerous_command_blocked() {
        let temp_dir = std::env::temp_dir();
        let res = SandboxTerminalRunner::validate_command("rm -rf /", &temp_dir, SecurityMode::Inherit);
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

        let result = SandboxTerminalRunner::execute(cmd, &temp_dir, SecurityMode::Inherit, Some(cancel_rx))
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
            SecurityMode::Inherit,
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
