use std::process::{Command, Stdio};
use std::io::{Write, Read};
use anyhow::{Result, anyhow};
use std::time::Duration;
use tracing::{info, warn, error};

use crate::ffi::cxp_ffi::ExtensionPayload;
use crate::parser::metadata_parser::EngineRules as CelEngineRules;

pub struct ProcessExecutor {}

impl Default for ProcessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessExecutor {
    pub fn new() -> Self {
        Self {}
    }

    /// Executes a given command (acting as an MCP Server or local script)
    /// Payload is sent via STDIN, and the JSON output is read from STDOUT.
    pub fn execute_with_rules(
        &self,
        command_str: &str,
        payload: &ExtensionPayload,
        rules: &CelEngineRules,
    ) -> Result<Vec<u8>> {
        // Parse the command string. E.g. "python server.py" or "node app.js"
        let mut parts = command_str.split_whitespace();
        let program = parts.next().ok_or_else(|| anyhow!("Empty process command"))?;
        let args: Vec<&str> = parts.collect();

        // Ensure subprocess execution is allowed
        if let Some(false) = rules.allow_subprocess {
            // Note: For MCP PROCESS envelope, this rule should ideally be true, or bypass it here 
            // since the envelope itself IS a process.
            warn!("ProcessExecutor invoked, but allow_subprocess is false. Proceeding as this is a PROCESS envelope.");
        }

        info!("🚀 [ProcessExecutor] Spawning: {} {:?}", program, args);

        let mut child_cmd = Command::new(program);
        child_cmd.args(&args);
        
        // Block environment variables if rules forbid it
        if let Some(false) = rules.allow_env_vars {
            child_cmd.env_clear();
        }

        child_cmd.stdin(Stdio::piped())
                 .stdout(Stdio::piped())
                 .stderr(Stdio::piped());

        let mut child = child_cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn process {}: {}", program, e))?;

        // Send payload to STDIN
        if let Some(mut stdin) = child.stdin.take() {
            let bytes = unsafe { payload.as_bytes() };
            stdin.write_all(bytes)
                .map_err(|e| anyhow!("Failed to write payload to STDIN: {}", e))?;
        }

        // We can enforce timeouts here using tokio if we were async, but since this is synchronous,
        // we can use a thread to wait with a timeout.
        let timeout_ms = rules.timeout_ms.unwrap_or(30000); // Default 30s timeout

        let (tx, rx) = std::sync::mpsc::channel();
        let pid = child.id();
        
        // Move stdout out of child
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();

        std::thread::spawn(move || {
            let mut out_data = Vec::new();
            let mut err_data = Vec::new();
            
            // Read all output (blocks until child closes STDOUT)
            let _ = stdout.read_to_end(&mut out_data);
            let _ = stderr.read_to_end(&mut err_data);
            
            let status = child.wait();
            let _ = tx.send((status, out_data, err_data));
        });

        match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
            Ok((Ok(status), out_data, err_data)) => {
                if status.success() {
                    Ok(out_data)
                } else {
                    let err_str = String::from_utf8_lossy(&err_data);
                    Err(anyhow!("Process exited with {}: {}", status, err_str))
                }
            }
            Ok((Err(e), _, _)) => {
                Err(anyhow!("Failed to wait on process: {}", e))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Kill process if it timeouts
                #[cfg(windows)]
                {
                    let _ = Command::new("taskkill").args(&["/F", "/PID", &pid.to_string()]).status();
                }
                #[cfg(not(windows))]
                {
                    let _ = Command::new("kill").args(&["-9", &pid.to_string()]).status();
                }
                Err(anyhow!("Process execution timed out after {} ms", timeout_ms))
            }
            Err(_) => Err(anyhow!("Internal thread communication error")),
        }
    }
}
