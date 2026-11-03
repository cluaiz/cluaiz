//! Opt-in: the Engine's MCP client against a real stdio MCP server. By default the official
//! reference server (`npx -y @modelcontextprotocol/server-everything`, needs Node.js and the network
//! once); `CLUAIZ_MCP_TEST_SERVER` may give another `{"command":…,"args":[…]}` with an `echo` tool.
//! `cargo test -p cluaiz-agents --test mcp_reference_server -- --ignored --nocapture`

use cluaiz_agents::mcp::McpClient;
use serde_json::{json, Value};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "starts a real MCP server (npx @modelcontextprotocol/server-everything by default)"]
async fn real_server_lists_and_calls_tools() -> anyhow::Result<()> {
    let execution: Value = match std::env::var("CLUAIZ_MCP_TEST_SERVER") {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => json!({"command":"npx","args":["-y","@modelcontextprotocol/server-everything"]}),
    };
    let package = std::env::temp_dir().join(format!("cluaiz-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&package)?;
    std::fs::write(
        package.join("package.json"),
        json!({"id":"mcp-test","description":"MCP test server","execution":execution}).to_string(),
    )?;
    let outcome = async {
        let tools = McpClient::list_tools(&package).await?;
        let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        println!("REAL MCP tools: {names:?}");
        anyhow::ensure!(names.contains(&"echo"), "no echo tool: {names:?}");

        let result = McpClient::call_tool(&package, "echo", json!({"message":"cluaiz"})).await?;
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        println!("REAL MCP echo: {text}");
        anyhow::ensure!(text.contains("cluaiz"), "{result}");

        // A tool-level failure (isError) is an error, not a result
        anyhow::ensure!(McpClient::call_tool(&package, "no-such-tool", json!({})).await.is_err());
        Ok(())
    }
    .await;
    McpClient::stop_server(&package).await;
    let _ = std::fs::remove_dir_all(&package);
    outcome
}
