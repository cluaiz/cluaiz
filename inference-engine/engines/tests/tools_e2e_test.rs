use std::fs;
use cluaiz_shared::environment::EnvironmentManager;
use engines::tools::{ToolsEngine, SessionToolBinding, ExecutionMode};

#[tokio::test]
async fn test_tools_end_to_end_lifecycle() {
    let env = EnvironmentManager::current();
    let skills_dir = env.skills_dir();
    let plugins_dir = env.plugins_dir();
    let mcp_dir = env.mcp_dir();

    // 1. Setup Dummy Test Directories
    let dummy_skill_dir = skills_dir.join("test-dummy-reviewer");
    let dummy_plugin_dir = plugins_dir.join("test-dummy-calc");
    let dummy_mcp_dir = mcp_dir.join("test-dummy-mcp");

    let _ = fs::create_dir_all(&dummy_skill_dir);
    let _ = fs::create_dir_all(&dummy_plugin_dir);
    let _ = fs::create_dir_all(&dummy_mcp_dir);

    // Create Dummy Skill (SKILL.md)
    let skill_md_content = r#"---
name: test-dummy-reviewer
version: 1.0.0
description: Autonomous dummy code review skill for testing
triggers:
  - "review this code"
  - "audit security"
execution_mode: auto
default_turns: 3
---

# Code Review Protocol
Always check for buffer overflows, memory safety, and DRY principles.
"#;
    fs::write(dummy_skill_dir.join("SKILL.md"), skill_md_content).unwrap();

    // Create Dummy Plugin (manifest-plugin.yaml)
    let plugin_yaml_content = r#"name: test-dummy-calc
version: 1.0.0
description: High performance math plugin
execution_mode: auto
default_turns: -1
execution:
  envelope: WASM
  binary_path: logic.wasm
"#;
    fs::write(dummy_plugin_dir.join("manifest-plugin.yaml"), plugin_yaml_content).unwrap();
    fs::write(dummy_plugin_dir.join("logic.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();

    // Create Real Subprocess Echo MCP (manifest-mcp.yaml)
    // Runs OS standard shell echoing standard JSON-RPC 2.0 response to stdio
    #[cfg(target_os = "windows")]
    let mcp_cmd = "cmd.exe";
    #[cfg(target_os = "windows")]
    let mcp_args = vec!["/c".to_string(), "echo {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"mcp_test_ok\"}".to_string()];

    #[cfg(not(target_os = "windows"))]
    let mcp_cmd = "sh";
    #[cfg(not(target_os = "windows"))]
    let mcp_args = vec!["-c".to_string(), "echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":\"mcp_test_ok\"}'".to_string()];

    let mcp_manifest_json = serde_yaml::to_string(&serde_json::json!({
        "name": "test-dummy-mcp",
        "version": "1.0.0",
        "description": "Model Context Protocol subprocess test bridge",
        "execution_mode": "manual",
        "default_turns": 0,
        "execution": {
            "command": mcp_cmd,
            "args": mcp_args
        }
    })).unwrap();

    fs::write(dummy_mcp_dir.join("manifest-mcp.yaml"), mcp_manifest_json).unwrap();

    // 2. Test Master Registry Sync
    let _ = ToolsEngine::registry().expect("ToolsRegistry should load and sync from disk");
    let all_tools = ToolsEngine::list_all_tools().expect("Should list all tools");

    println!("📋 [Test] Total discovered tools: {}", all_tools.len());

    // Verify Dummy Skill in Registry
    let skill_tool = ToolsEngine::get_tool("test-dummy-reviewer").expect("Query tool").expect("Skill must exist");
    assert_eq!(skill_tool.category, "skill");
    assert_eq!(skill_tool.default_turns, 3);
    assert_eq!(skill_tool.execution_mode, ExecutionMode::Auto);
    assert!(skill_tool.semantic_triggers.contains(&"review this code".to_string()));

    // Verify Dummy Plugin in Registry
    let plugin_tool = ToolsEngine::get_tool("test-dummy-calc").expect("Query tool").expect("Plugin must exist");
    assert_eq!(plugin_tool.category, "plugin");
    assert_eq!(plugin_tool.default_turns, -1);

    // Verify Dummy MCP in Registry
    let mcp_tool = ToolsEngine::get_tool("test-dummy-mcp").expect("Query tool").expect("MCP must exist");
    assert_eq!(mcp_tool.category, "mcp");
    assert_eq!(mcp_tool.execution_mode, ExecutionMode::Manual);
    assert_eq!(mcp_tool.default_turns, 0);

    // 3. Test Semantic Trigger Matching & Skill Instructions
    let matched = ToolsEngine::match_skills("Can you please review this code for me?");
    assert!(matched.contains(&"test-dummy-reviewer".to_string()), "Skill must trigger on keyword match");

    let instructions = ToolsEngine::get_skill_instructions("test-dummy-reviewer").expect("Must extract instructions");
    assert!(instructions.contains("Always check for buffer overflows"), "Instructions must match body of SKILL.md");

    // 4. Test REAL Subprocess MCP Execution via McpClient & ToolsEngine
    println!("🔌 [Test] Invoking real MCP subprocess over stdio...");
    let mcp_res = engines::tools::McpClient::call_tool(&dummy_mcp_dir, "ping", serde_json::json!({})).await
        .expect("Real MCP subprocess should execute and return parsed result");
    assert_eq!(mcp_res.as_str().unwrap(), "mcp_test_ok", "Subprocess must return parsed JSON-RPC result 'mcp_test_ok'");

    // 5. Test Unified DRY Dispatcher ToolsEngine::execute_tool_by_name
    let unified_mcp_res = ToolsEngine::execute_tool_by_name("mcp", "test-dummy-mcp", Some("ping"), "{}").await
        .expect("Unified dispatcher should execute MCP tool");
    assert_eq!(unified_mcp_res, "mcp_test_ok");

    // 6. Test Session Tool Lifecycle & Turn Decrements
    let session_id = "test_sess_real_mcp";
    let bindings = vec![
        SessionToolBinding {
            id: "test-dummy-reviewer".to_string(),
            turns: 2,
        },
        SessionToolBinding {
            id: "test-dummy-calc".to_string(),
            turns: -1,
        },
        SessionToolBinding {
            id: "test-dummy-mcp".to_string(),
            turns: 0,
        },
    ];

    ToolsEngine::update_session_tools(session_id, bindings, vec![]);
    let active_ids = ToolsEngine::get_active_tool_ids_for_session(session_id);
    assert_eq!(active_ids.len(), 3);

    // Turn 1 Decrement
    ToolsEngine::decrement_session_turns(session_id);
    let active_ids_turn1 = ToolsEngine::get_active_tool_ids_for_session(session_id);
    assert!(!active_ids_turn1.contains(&"test-dummy-mcp".to_string()), "0-turn tool must be purged after turn 1");
    assert!(active_ids_turn1.contains(&"test-dummy-calc".to_string()), "-1 permanent tool must remain");
    assert!(active_ids_turn1.contains(&"test-dummy-reviewer".to_string()), "Countdown tool must remain");

    // Turn 2 Decrement
    ToolsEngine::decrement_session_turns(session_id);
    let active_ids_turn2 = ToolsEngine::get_active_tool_ids_for_session(session_id);
    assert!(!active_ids_turn2.contains(&"test-dummy-reviewer".to_string()), "Expired tool must be purged");
    assert!(active_ids_turn2.contains(&"test-dummy-calc".to_string()), "-1 permanent tool must still remain");

    // 7. Cleanup Test Directories
    let _ = fs::remove_dir_all(dummy_skill_dir);
    let _ = fs::remove_dir_all(dummy_plugin_dir);
    let _ = fs::remove_dir_all(dummy_mcp_dir);
    let _ = ToolsEngine::registry();

    println!("✅ [Test] All Tools E2E tests (including real MCP subprocess execution) passed successfully!");
}
