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
    let dummy_skill_dir = skills_dir.join("dummy-reviewer");
    let dummy_plugin_dir = plugins_dir.join("dummy-calc");
    let dummy_mcp_dir = mcp_dir.join("dummy-git-mcp");

    fs::create_dir_all(&dummy_skill_dir).unwrap();
    fs::create_dir_all(&dummy_plugin_dir).unwrap();
    fs::create_dir_all(&dummy_mcp_dir).unwrap();

    // Create Dummy Skill (SKILL.md)
    let skill_md_content = r#"---
name: dummy-reviewer
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
    let plugin_yaml_content = r#"name: dummy-calc
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

    // Create Dummy MCP (manifest-mcp.yaml)
    let mcp_yaml_content = r#"name: dummy-git-mcp
version: 1.0.0
description: Model Context Protocol Git bridge
execution_mode: manual
default_turns: 0
execution:
  command: git-mcp
  args: ["--stdio"]
"#;
    fs::write(dummy_mcp_dir.join("manifest-mcp.yaml"), mcp_yaml_content).unwrap();

    // 2. Test Master Registry Sync
    let registry = ToolsEngine::registry().expect("ToolsRegistry should load and sync from disk");
    let all_tools = ToolsEngine::list_all_tools().expect("Should list all tools");

    println!("📋 [Test] Total discovered tools: {}", all_tools.len());
    for tool in &all_tools {
        println!("  - Tool: {} | Category: {} | Mode: {:?} | Turns: {}",
            tool.id, tool.category, tool.execution_mode, tool.default_turns);
    }

    // Verify Dummy Skill in Registry
    let skill_tool = ToolsEngine::get_tool("dummy-reviewer").expect("Query tool").expect("Skill must exist");
    assert_eq!(skill_tool.category, "skill");
    assert_eq!(skill_tool.default_turns, 3);
    assert_eq!(skill_tool.execution_mode, ExecutionMode::Auto);
    assert!(skill_tool.semantic_triggers.contains(&"review this code".to_string()));

    // Verify Dummy Plugin in Registry
    let plugin_tool = ToolsEngine::get_tool("dummy-calc").expect("Query tool").expect("Plugin must exist");
    assert_eq!(plugin_tool.category, "plugin");
    assert_eq!(plugin_tool.default_turns, -1);

    // Verify Dummy MCP in Registry
    let mcp_tool = ToolsEngine::get_tool("dummy-git-mcp").expect("Query tool").expect("MCP must exist");
    assert_eq!(mcp_tool.category, "mcp");
    assert_eq!(mcp_tool.execution_mode, ExecutionMode::Manual);
    assert_eq!(mcp_tool.default_turns, 0);

    // 3. Test Semantic Trigger Matching
    let matched = ToolsEngine::match_skills("Can you please review this code for me?");
    assert!(matched.contains(&"dummy-reviewer".to_string()), "Skill must trigger on keyword match");

    // 4. Test Skill Instructions Extraction
    let instructions = ToolsEngine::get_skill_instructions("dummy-reviewer").expect("Must extract instructions");
    assert!(instructions.contains("Always check for buffer overflows"), "Instructions must match body of SKILL.md");

    // 5. Test Session Tool Lifecycle & Turn Decrements
    let session_id = "test_sess_999";
    let bindings = vec![
        SessionToolBinding {
            id: "dummy-reviewer".to_string(),
            turns: 2, // 2 turns countdown
        },
        SessionToolBinding {
            id: "dummy-calc".to_string(),
            turns: -1, // Permanent all-time
        },
        SessionToolBinding {
            id: "dummy-git-mcp".to_string(),
            turns: 0, // Ephemeral 1-turn (auto-purge on first turn end)
        },
    ];

    ToolsEngine::update_session_tools(session_id, bindings, vec![]);
    let active_ids = ToolsEngine::get_active_tool_ids_for_session(session_id);
    assert_eq!(active_ids.len(), 3);

    // First Turn Decrement
    ToolsEngine::decrement_session_turns(session_id);
    let active_ids_turn1 = ToolsEngine::get_active_tool_ids_for_session(session_id);
    // dummy-git-mcp had 0 turns -> purged
    assert!(!active_ids_turn1.contains(&"dummy-git-mcp".to_string()), "0-turn tool must be purged");
    assert!(active_ids_turn1.contains(&"dummy-calc".to_string()), "-1 permanent tool must remain");
    assert!(active_ids_turn1.contains(&"dummy-reviewer".to_string()), "Countdown tool must remain");

    // Check countdown turn count
    let sess_tools = ToolsEngine::get_session_tools(session_id);
    let rev_tool = sess_tools.iter().find(|t| t.id == "dummy-reviewer").unwrap();
    assert_eq!(rev_tool.turns, 1, "Remaining turns must decrement from 2 to 1");

    // Second Turn Decrement (1 -> 0 -> expires)
    ToolsEngine::decrement_session_turns(session_id);
    let active_ids_turn2 = ToolsEngine::get_active_tool_ids_for_session(session_id);
    assert!(!active_ids_turn2.contains(&"dummy-reviewer".to_string()), "Expired tool must be purged");
    assert!(active_ids_turn2.contains(&"dummy-calc".to_string()), "-1 permanent tool must still remain");

    // 6. Test Enable/Disable Toggle
    ToolsEngine::set_tool_enabled("dummy-calc", false).expect("Disable tool");
    let disabled_calc = ToolsEngine::get_tool("dummy-calc").unwrap().unwrap();
    assert_eq!(disabled_calc.enabled, false);

    ToolsEngine::set_tool_enabled("dummy-calc", true).expect("Re-enable tool");
    let enabled_calc = ToolsEngine::get_tool("dummy-calc").unwrap().unwrap();
    assert_eq!(enabled_calc.enabled, true);

    // 7. Test Plugin Execution (WASM Simulated Envelope)
    let exec_res = ToolsEngine::execute_plugin_by_name("dummy-calc", b"{\"action\": \"add\", \"a\": 2, \"b\": 3}").expect("Execution should succeed");
    assert!(!exec_res.is_empty(), "Plugin response must not be empty");

    // 8. Test Real Package Installation from cluaiz-hub
    println!("📦 [Test] Installing real 'cluaiz-search' plugin from cluaiz-hub...");
    engines::tools::ToolHubInstaller::install_component("plugin", "cluaiz-search").await.expect("Install cluaiz-search from hub");
    
    let search_tool = ToolsEngine::get_tool("cluaiz-search").expect("Query tool").expect("cluaiz-search must be registered in ToolsRegistry");
    assert_eq!(search_tool.category, "plugin");
    assert!(search_tool.semantic_triggers.contains(&"search".to_string()), "Must contain 'search' trigger from manifest-plugin.yaml");
    println!("✅ [Test] 'cluaiz-search' installed and registered successfully with triggers: {:?}", search_tool.semantic_triggers);

    // 8b. Test Invalid Version Handling
    println!("🧪 [Test] Testing invalid version request 'cluaiz-search@9.9.9'...");
    let invalid_res = engines::tools::ToolHubInstaller::install_component("plugin", "cluaiz-search@9.9.9").await;
    assert!(invalid_res.is_err(), "Invalid version must return an error");
    let err_msg = invalid_res.err().unwrap().to_string();
    println!("✅ [Test] Correctly caught invalid version error: {}", err_msg);
    assert!(err_msg.contains("Version '9.9.9' not found"), "Error message must state version not found");

    // Clean up cluaiz-search
    let _ = engines::tools::ToolHubInstaller::remove_component("plugin", "cluaiz-search").await;

    // 9. Clean up Test Artifacts
    let _ = fs::remove_dir_all(dummy_skill_dir);
    let _ = fs::remove_dir_all(dummy_plugin_dir);
    let _ = fs::remove_dir_all(dummy_mcp_dir);

    // Resync registry clean state
    let _ = ToolsEngine::registry();
    println!("✅ [Test] All Tools E2E tests passed successfully!");
}
