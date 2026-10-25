//! `cluaiz mcp-server`: the running Engine as an MCP server over stdio, for clients that launch
//! one (Claude Desktop, Cursor: `"command": "cluaiz", "args": ["mcp-server"]`). Every stdin line is one
//! JSON-RPC message for the Engine's `/mcp`; every reply is one stdout line. Nothing else goes to
//! stdout (logs are on stderr), as the MCP stdio transport requires.

use color_eyre::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A tool call can load a model or read a large file
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// One message to the Engine; `None` when there is nothing to answer (a notification)
async fn forward(line: &str) -> Option<Value> {
    let id = serde_json::from_str::<Value>(line).ok().and_then(|m| m.get("id").cloned());
    let sent = super::server::request_with_timeout(reqwest::Method::POST, "/mcp", CALL_TIMEOUT)
        .header("content-type", "application/json")
        .body(line.to_string())
        .send()
        .await;
    let failure = |message: String| {
        eprintln!("cluaiz mcp-server: {message}");
        id.clone().map(|id| json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": message } }))
    };
    match sent {
        Ok(response) if response.status() == reqwest::StatusCode::ACCEPTED => None,
        Ok(response) => match response.json::<Value>().await {
            Ok(reply) => Some(reply),
            Err(e) => failure(format!("unreadable reply from the Engine: {e}")),
        },
        Err(e) => failure(format!("the Engine at {} is not reachable (is `cluaiz serve` running?): {e}", super::server::base_url())),
    }
}

pub async fn run() -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = forward(&line).await {
            stdout.write_all(format!("{reply}\n").as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}
