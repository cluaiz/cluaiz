use std::fs;
use serde_json::json;
use engines::tools::{McpClient, ToolsEngine, ToolPromptCompiler, skills::SkillParser};

#[tokio::test]
async fn test_official_anthropic_filesystem_mcp() {
    let temp_dir = std::env::temp_dir().join("cluaiz_official_anthropic_mcp_test");
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Create a test file that the official Anthropic MCP server will read
    let sample_file_path = temp_dir.join("market_proof.txt");
    fs::write(&sample_file_path, "Official Anthropic MCP Server verified inside Cluaiz Engine!").unwrap();

    // 2. Write package.json using official Anthropic MCP server package via npx
    let package_json = json!({
        "name": "official-anthropic-fs",
        "version": "1.0.0",
        "description": "Official Anthropic Model Context Protocol Filesystem Server",
        "mcp": {
            "protocolVersion": "2024-11-05",
            "transport": "stdio",
            "execution": {
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem", temp_dir.to_str().unwrap()]
            }
        }
    });
    fs::write(temp_dir.join("package.json"), serde_json::to_string_pretty(&package_json).unwrap()).unwrap();

    println!("🔌 [Test] Launching official Anthropic @modelcontextprotocol/server-filesystem via stdio JSON-RPC...");

    // 3. Dynamic Tool Discovery (tools/list)
    let tools = McpClient::list_tools(&temp_dir).await
        .expect("McpClient should connect and discover tools from official Anthropic MCP server");

    println!("📋 [Test] Discovered {} tools from official Anthropic MCP server", tools.len());
    assert!(!tools.is_empty(), "Official Anthropic MCP server must return at least 1 tool");

    let tool_names: Vec<&str> = tools.iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    println!("🔍 [Test] Official tools exposed: {:?}", tool_names);
    assert!(tool_names.contains(&"read_file"), "Official MCP server must expose 'read_file'");
    assert!(tool_names.contains(&"list_directory"), "Official MCP server must expose 'list_directory'");

    // 4. Test Reverse Target Resolution (FR-3)
    let resolved = ToolsEngine::resolve_function_target("read_file", &["official-anthropic-fs".to_string()]);
    assert!(resolved.is_some(), "Reverse resolver must resolve 'read_file' to official-anthropic-fs");
    let target = resolved.unwrap();
    assert_eq!(target.category, "mcp");
    assert_eq!(target.component_id, "official-anthropic-fs");
    assert_eq!(target.sub_function, Some("read_file".to_string()));

    // 5. Test Live Execution (tools/call)
    println!("⚡ [Test] Calling official tool 'read_file' for 'market_proof.txt'...");
    let read_result = McpClient::call_tool(
        &temp_dir,
        "read_file",
        json!({ "path": sample_file_path.to_str().unwrap() })
    ).await.expect("McpClient::call_tool should successfully execute on official Anthropic MCP server");

    println!("✅ [Test] Official MCP server response: {:?}", read_result);
    let result_str = serde_json::to_string(&read_result).unwrap();
    assert!(
        result_str.contains("Official Anthropic MCP Server verified inside Cluaiz Engine!"),
        "The official MCP server must return the exact file contents from disk"
    );

    // 6. Test Standard Response Formatter (FR-4)
    let formatted_xml = ToolsEngine::format_tool_response("read_file", &read_result);
    assert!(formatted_xml.contains("<tool_response>"));
    assert!(formatted_xml.contains("\"name\": \"read_file\""));

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
    println!("🎉 [Test] Official Anthropic MCP server test completed with 100% success!");
}

#[test]
fn test_official_community_agent_skill_format() {
    let temp_dir = std::env::temp_dir().join("cluaiz_community_skill_test");
    let _ = fs::create_dir_all(&temp_dir);

    let skill_md_content = r#"---
name: community-security-audit
version: 2.1.0
description: Official community agent skill for detecting SQL injections and buffer overflows
triggers:
  keywords: ["audit security", "check vulnerability", "code review"]
execution_mode: auto
default_turns: 3
---

# Enterprise Security Audit Workflow
1. Disallow raw string concatenation in SQL queries.
2. Verify all unbounded memory buffers are bounded.
3. Validate JSON schemas before passing to FFI.
"#;

    let skill_path = temp_dir.join("SKILL.md");
    fs::write(&skill_path, skill_md_content).unwrap();

    let parsed = SkillParser::parse_file(&skill_path)
        .expect("SkillParser must parse official open-source SKILL.md format");

    assert_eq!(parsed.metadata.name, "community-security-audit");
    assert_eq!(parsed.metadata.version, "2.1.0");
    assert!(parsed.prompt_instructions.contains("Enterprise Security Audit Workflow"));
    assert!(parsed.prompt_instructions.contains("Disallow raw string concatenation"));

    let _ = fs::remove_dir_all(&temp_dir);
    println!("🎉 [Test] Official community SKILL.md format test passed!");
}

