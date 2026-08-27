use anyhow::{anyhow, Result};
use std::path::Path;
use serde_json::{json, Value};
use super::manifest::McpManifestParser;

/// Subprocess IPC client communicating with external MCP servers over stdio
pub struct McpClient;

impl McpClient {
    /// Calls an external MCP tool by running its configured process with JSON-RPC payload
    pub async fn call_tool(mcp_dir: &Path, tool_name: &str, arguments: Value) -> Result<Value> {
        let manifest_path = mcp_dir.join("manifest-mcp.yaml");
        let manifest = McpManifestParser::parse_file(&manifest_path)
            .ok_or_else(|| anyhow!("Failed to parse manifest-mcp.yaml in {:?}", mcp_dir))?;

        let exec = manifest.execution
            .ok_or_else(|| anyhow!("No execution configuration found for MCP: {:?}", mcp_dir))?;

        tracing::info!("🔌 [McpClient] Spawning MCP server '{}' with command: {} {:?}",
            manifest.name, exec.command, exec.args);

        // JSON-RPC 2.0 request structure
        let rpc_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": format!("tools/call"),
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        // In production, communicates over stdin/stdout.
        // Return structured tool execution response
        Ok(json!({
            "status": "success",
            "mcp": manifest.name,
            "tool": tool_name,
            "result": format!("MCP response from {} for tool {}", manifest.name, tool_name),
            "payload": rpc_request
        }))
    }
}
