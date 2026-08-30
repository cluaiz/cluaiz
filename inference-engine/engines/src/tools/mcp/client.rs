use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;
use super::manifest::McpManifestParser;

/// Subprocess IPC client communicating with external MCP servers over stdio with full JSON-RPC 2.0 protocol handshake
pub struct McpClient;

impl McpClient {
    /// Calls an external MCP tool by running its configured process with spec-compliant handshake over stdio
    pub async fn call_tool(mcp_dir: &Path, tool_name: &str, arguments: Value) -> Result<Value> {
        let manifest_path = mcp_dir.join("manifest-mcp.yaml");
        let manifest = McpManifestParser::parse_file(&manifest_path)
            .ok_or_else(|| anyhow!("Failed to parse manifest-mcp.yaml in {:?}", mcp_dir))?;

        let exec = manifest.execution
            .ok_or_else(|| anyhow!("No execution configuration found for MCP in {:?}", mcp_dir))?;

        if exec.command.trim().is_empty() {
            return Err(anyhow!("MCP execution command is empty for {:?}", mcp_dir));
        }

        tracing::info!("🔌 [McpClient] Spawning MCP server '{}' via stdio: {} {:?}",
            manifest.name, exec.command, exec.args);

        let mut cmd = Command::new(&exec.command);
        cmd.args(&exec.args)
            .current_dir(mcp_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &exec.env {
            cmd.env(k, v);
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn MCP server '{}' ({}): {}", manifest.name, exec.command, e))?;

        let mut stdin = child.stdin.take()
            .ok_or_else(|| anyhow!("Failed to capture stdin for MCP server '{}'", manifest.name))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow!("Failed to capture stdout for MCP server '{}'", manifest.name))?;
        let stderr = child.stderr.take();

        // Spawn background task to drain stderr to prevent pipe blocking / deadlock
        if let Some(err_pipe) = stderr {
            tokio::spawn(async move {
                let mut lines = BufReader::new(err_pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!("[MCP Server stderr]: {}", line);
                }
            });
        }

        let mut reader = BufReader::new(stdout).lines();
        let timeout_duration = Duration::from_secs(30);

        // ── STEP 1: Mandatory MCP `initialize` Handshake ──
        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "roots": { "listChanged": true },
                    "sampling": {}
                },
                "clientInfo": {
                    "name": "cluaiz-engine",
                    "version": "1.0.0"
                }
            }
        });

        let mut init_line = serde_json::to_string(&init_req)?;
        init_line.push('\n');
        stdin.write_all(init_line.as_bytes()).await
            .map_err(|e| anyhow!("Failed to send initialize to MCP stdin: {}", e))?;
        stdin.flush().await
            .map_err(|e| anyhow!("Failed to flush MCP stdin on initialize: {}", e))?;

        // Read initialize response
        let _init_response = timeout(timeout_duration, async {
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                    if val.get("id").and_then(|id| id.as_i64()) == Some(1) {
                        return Ok(val);
                    }
                }
            }
            Err(anyhow!("MCP server exited before responding to initialize"))
        }).await
        .map_err(|_| anyhow!("Timeout (30s) waiting for MCP server initialize response"))??;

        // ── STEP 2: Send `notifications/initialized` ──
        let notify_init = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let mut notify_line = serde_json::to_string(&notify_init)?;
        notify_line.push('\n');
        stdin.write_all(notify_line.as_bytes()).await
            .map_err(|e| anyhow!("Failed to send initialized notification: {}", e))?;
        stdin.flush().await
            .map_err(|e| anyhow!("Failed to flush initialized notification: {}", e))?;

        // ── STEP 3: Dispatch `tools/call` ──
        let rpc_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let mut rpc_line = serde_json::to_string(&rpc_request)?;
        rpc_line.push('\n');
        stdin.write_all(rpc_line.as_bytes()).await
            .map_err(|e| anyhow!("Failed to write tools/call request to MCP stdin: {}", e))?;
        stdin.flush().await
            .map_err(|e| anyhow!("Failed to flush tools/call request: {}", e))?;

        // Asynchronously read response line for tools/call
        let response_json = timeout(timeout_duration, async {
            while let Ok(Some(line)) = reader.next_line().await {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                    if val.get("id").and_then(|id| id.as_i64()) == Some(2) {
                        return Ok(val);
                    }
                }
            }
            Err(anyhow!("MCP server exited without returning tools/call response"))
        }).await
        .map_err(|_| anyhow!("Timeout (30s) waiting for MCP server '{}' tools/call response", manifest.name))??;

        // Clean up child process gracefully
        drop(stdin);
        let _ = child.kill().await;

        // Verify JSON-RPC error vs result
        if let Some(err_val) = response_json.get("error") {
            return Err(anyhow!("MCP tool error: {}", err_val));
        }

        if let Some(result_val) = response_json.get("result") {
            Ok(result_val.clone())
        } else {
            Ok(response_json)
        }
    }
}
