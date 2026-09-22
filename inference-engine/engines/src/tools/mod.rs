pub mod execution;
pub mod installer;
pub mod lifecycle;
pub mod mcp;
pub mod plugins;
pub mod prompt_compiler;
pub mod registry;
pub mod skills;
pub mod telemetry;

use std::path::Path;
use anyhow::Result;
use serde_json::Value;

pub use execution::{CancelHandle, EnvironmentResolver, ExecutionPolicy, PolicyDecision, SandboxTerminalRunner, TerminalRunResult};
pub use installer::{EnvironmentStatus, RuntimeEnvironmentManager, RuntimeType, ToolsInstaller, ToolHubInstaller};
pub use lifecycle::{SessionToolBinding, SessionToolManager, TurnLifecycleEngine};
pub use mcp::{McpClient, McpManifest};
pub use plugins::{PluginExecutor, PluginManifest};
pub use prompt_compiler::{CompiledPromptTools, ResolvedToolTarget, ToolPromptCompiler};
pub use registry::{ExecutionMode, LoadStrategy, SecurityMode, ToolCategory, ToolEntry, ToolsRegistry};
pub use skills::{ParsedSkill, SkillParser, SkillRouter};
pub use telemetry::{ContextBreakdown, ContextTracker, SystemContextTelemetry};


/// Unified Public Facade: ToolsEngine
/// Single Domain Sovereign Entrypoint for all Tools, Skills, Plugins, and MCP bridges
pub struct ToolsEngine;

impl ToolsEngine {
    /// Loads master tools registry from `~/.cluaiz/engine/config/tools_registry.json`
    pub fn registry() -> Result<ToolsRegistry> {
        ToolsRegistry::load()
    }

    /// Returns all registered tools across all categories
    pub fn list_all_tools() -> Result<Vec<ToolEntry>> {
        let reg = Self::registry()?;
        Ok(reg.list_tools())
    }

    /// Retrieves a specific tool by ID
    pub fn get_tool(id: &str) -> Result<Option<ToolEntry>> {
        let reg = Self::registry()?;
        Ok(reg.get_tool(id).cloned())
    }

    /// Enables or disables a tool
    pub fn set_tool_enabled(id: &str, enabled: bool) -> Result<()> {
        let mut reg = Self::registry()?;
        reg.set_tool_enabled(id, enabled)
    }

    /// Configures execution mode (Auto / Manual)
    pub fn set_tool_execution_mode(id: &str, mode: ExecutionMode) -> Result<()> {
        let mut reg = Self::registry()?;
        reg.set_tool_execution_mode(id, mode)
    }

    /// Configures default turn lifetime
    pub fn set_tool_default_turns(id: &str, turns: i32) -> Result<()> {
        let mut reg = Self::registry()?;
        reg.set_tool_default_turns(id, turns)
    }

    /// Configures security mode (full_access / sandboxed / strict)
    pub fn set_tool_security_mode(id: &str, mode: SecurityMode) -> Result<()> {
        let mut reg = Self::registry()?;
        if let Some(tool) = reg.installed_tools.get_mut(id) {
            tool.security_mode = mode;
            reg.save()?;
        }
        Ok(())
    }

    /// Downloads and installs a tool from Cluaiz Tools into `~/.cluaiz/tools/{skills,plugins,mcp}`
    pub async fn install_tool(category: &str, tool_id: &str) -> Result<()> {
        ToolsInstaller::install_component(category, tool_id).await
    }

    /// Removes an installed tool from filesystem and syncs registry
    pub async fn remove_tool(category: &str, tool_id: &str) -> Result<()> {
        ToolsInstaller::remove_component(category, tool_id).await
    }

    /// Returns active tools bound to a specific chat session
    pub fn get_session_tools(session_id: &str) -> Vec<SessionToolBinding> {
        SessionToolManager::get_session_tools(session_id)
    }

    /// Returns active tool IDs for a session
    pub fn get_active_tool_ids_for_session(session_id: &str) -> Vec<String> {
        SessionToolManager::get_active_tool_ids(session_id)
    }

    /// Updates session tool bindings
    pub fn update_session_tools(session_id: &str, tools: Vec<SessionToolBinding>, detach: Vec<String>) -> Vec<SessionToolBinding> {
        SessionToolManager::update_session_tools(session_id, tools, detach)
    }

    /// Decrements turns upon chat response completion and purges expired tools
    pub fn decrement_session_turns(session_id: &str) {
        TurnLifecycleEngine::decrement_turns(session_id);
    }

    /// Computes real-time context token breakdown and KV-cache telemetry
    pub fn compute_telemetry(active_model_id: &str, session_id: &str, active_tool_ids: &[String], prompt_len: usize, history_len: usize, system_prompt_len: usize, generated_tokens: usize) -> SystemContextTelemetry {
        ContextTracker::compute_telemetry(active_model_id, session_id, active_tool_ids, prompt_len, history_len, system_prompt_len, generated_tokens)
    }

    /// Matches user query against skill keyword triggers
    pub fn match_skills(query: &str) -> Vec<String> {
        let router = SkillRouter::new();
        router.match_query(query)
    }

    /// Retrieves instructions for a skill to inject into LLM system prompt
    pub fn get_skill_instructions(skill_id: &str) -> Option<String> {
        let router = SkillRouter::new();
        router.get_instructions(skill_id).map(|s| s.to_string())
    }

    /// Executes a WASM or Native plugin by path
    pub fn execute_plugin(plugin_dir: &Path, payload: &[u8]) -> Result<Vec<u8>> {
        PluginExecutor::execute(plugin_dir, payload)
    }

    /// Executes a WASM or Native plugin by name resolving path from ~/.cluaiz/tools/plugins
    pub fn execute_plugin_by_name(plugin_name: &str, payload: &[u8]) -> Result<Vec<u8>> {
        let env = engine_core::environment::EnvironmentManager::current();
        let plugin_dir = env.plugins_dir().join(plugin_name);
        if plugin_dir.exists() {
            return Self::execute_plugin(&plugin_dir, payload);
        }
        let alt_dir = env.global_dir.join("plugins").join(plugin_name);
        if alt_dir.exists() {
            return Self::execute_plugin(&alt_dir, payload);
        }
        if let Ok(reg) = Self::registry() {
            if let Some(entry) = reg.get_tool(plugin_name) {
                let p = Path::new(&entry.local_dir);
                if p.exists() {
                    return Self::execute_plugin(p, payload);
                }
            }
        }
        Err(anyhow::anyhow!("Plugin '{}' not found in {:?}", plugin_name, plugin_dir))
    }

    /// Calls an external MCP tool via subprocess IPC
    pub async fn call_mcp(mcp_dir: &Path, tool_name: &str, arguments: Value) -> Result<Value> {
        McpClient::call_tool(mcp_dir, tool_name, arguments).await
    }

    /// Calls an external MCP tool by name resolving path from ~/.cluaiz/tools/mcp
    pub async fn call_mcp_by_name(mcp_name: &str, tool_name: &str, arguments: Value) -> Result<Value> {
        let env = engine_core::environment::EnvironmentManager::current();
        let mcp_dir = env.mcp_dir().join(mcp_name);
        if mcp_dir.exists() {
            return Self::call_mcp(&mcp_dir, tool_name, arguments).await;
        }
        let alt_dir = env.global_dir.join("mcp").join(mcp_name);
        if alt_dir.exists() {
            return Self::call_mcp(&alt_dir, tool_name, arguments).await;
        }
        if let Ok(reg) = Self::registry() {
            if let Some(entry) = reg.get_tool(mcp_name) {
                let p = Path::new(&entry.local_dir);
                if p.exists() {
                    return Self::call_mcp(p, tool_name, arguments).await;
                }
            }
        }
        Err(anyhow::anyhow!("MCP server '{}' not found in {:?}", mcp_name, mcp_dir))
    }

    /// Discovers MCP tools dynamically by server name
    pub async fn list_mcp_tools_by_name(mcp_name: &str) -> Result<Vec<Value>> {
        let env = engine_core::environment::EnvironmentManager::current();
        let mcp_dir = env.mcp_dir().join(mcp_name);
        if mcp_dir.exists() {
            McpClient::list_tools(&mcp_dir).await
        } else {
            let alt_dir = env.global_dir.join("mcp").join(mcp_name);
            if alt_dir.exists() {
                McpClient::list_tools(&alt_dir).await
            } else {
                Err(anyhow::anyhow!("MCP server '{}' not found in {:?}", mcp_name, mcp_dir))
            }
        }
    }

    /// Compiles prompt tools and instructions for an active session ID
    pub async fn compile_prompt_tools_for_session(session_id: &str) -> CompiledPromptTools {
        ToolPromptCompiler::compile_for_session(session_id).await
    }

    /// Compiles active session tools schema directly into ChatML XML block (FR-1 spec)
    pub async fn compile_prompt_schema(session_id: &str) -> String {
        ToolPromptCompiler::compile_for_session(session_id).await.tools_xml
    }

    /// Compiles prompt tools and instructions for a specific list of tool IDs
    pub async fn compile_prompt_tools(tool_ids: &[String]) -> CompiledPromptTools {
        ToolPromptCompiler::compile_tools(tool_ids).await
    }

    /// Resolves an emitted function name to its host component target (FR-3)
    pub fn resolve_function_target(fn_name: &str, active_tool_ids: &[String]) -> Option<ResolvedToolTarget> {
        ToolPromptCompiler::resolve_target(fn_name, active_tool_ids)
    }

    /// Resolves an emitted function name for an active chat session (FR-3)
    pub fn resolve_function_target_for_session(session_id: &str, fn_name: &str) -> Option<ResolvedToolTarget> {
        let active_ids = Self::get_active_tool_ids_for_session(session_id);
        ToolPromptCompiler::resolve_target(fn_name, &active_ids)
    }

    /// Formats a tool execution output into standard Market ChatML `<tool_response>` block (FR-4)
    pub fn format_tool_response(name: &str, content: &Value) -> String {
        let json_str = serde_json::to_string(content).unwrap_or_else(|_| "{}".to_string());
        format!("<tool_response>\n{{\"name\": \"{}\", \"content\": {}}}\n</tool_response>", name, json_str)
    }

    /// Formats a raw tool output string into standard Market ChatML `<tool_response>` block (FR-4)
    pub fn format_tool_response_raw(name: &str, raw_content: &str) -> String {
        let content_json: Value = serde_json::from_str(raw_content).unwrap_or_else(|_| {
            serde_json::json!({ "output": raw_content })
        });
        Self::format_tool_response(name, &content_json)
    }

    /// Unified DRY executor across plugins, MCP servers, skills, and sandboxed terminal commands
    pub async fn execute_tool_by_name(category: &str, name: &str, tool_name: Option<&str>, payload: &str) -> Result<String> {
        let is_terminal = category == "terminal" 
            || name == "run_command" 
            || name == "terminal" 
            || name == "bash"
            || tool_name.map_or(false, |t| t == "run_command" || t == "bash" || t == "terminal");

        if is_terminal {
            let parsed_payload: Value = serde_json::from_str(payload).unwrap_or_else(|_| serde_json::json!({ "command": payload }));
            let command = parsed_payload.get("command")
                .or_else(|| parsed_payload.get("cmd"))
                .and_then(|c| c.as_str())
                .unwrap_or(payload);

            let cwd_path = parsed_payload.get("cwd")
                .and_then(|c| c.as_str())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));

            let sec_mode = Self::get_tool(name)
                .ok()
                .flatten()
                .map(|t| t.security_mode)
                .unwrap_or(SecurityMode::Sandboxed);

            let stdin_input = parsed_payload.get("stdin")
                .or_else(|| parsed_payload.get("input"))
                .and_then(|i| i.as_str());

            let timeout_opt = parsed_payload.get("timeout")
                .and_then(|t| t.as_u64());

            let venv_dir = RuntimeEnvironmentManager::tool_venv_dir(name);
            let venv_opt = if venv_dir.exists() { Some(venv_dir.as_path()) } else { None };

            let run_result = SandboxTerminalRunner::execute_advanced(command, &cwd_path, sec_mode, venv_opt, stdin_input, timeout_opt, None).await?;
            let output_json = serde_json::to_string_pretty(&run_result)?;
            return Ok(output_json);
        }

        match category {
            "mcp" => {
                let target_tool = tool_name.unwrap_or(name);
                let args: Value = serde_json::from_str(payload).unwrap_or_else(|_| serde_json::json!({ "raw": payload }));

                // Auto-provision environment if local tool directory exists
                if let Ok(Some(entry)) = Self::get_tool(name) {
                    let tool_dir = std::path::PathBuf::from(&entry.local_dir);
                    if tool_dir.exists() && !RuntimeEnvironmentManager::is_environment_ready(&tool_dir, name) {
                        tracing::info!("📦 [ToolsEngine] Auto-provisioning isolated runtime for MCP '{}'", name);
                        let _ = RuntimeEnvironmentManager::provision_environment(&tool_dir, name).await;
                    }
                }

                let result_val = Self::call_mcp_by_name(name, target_tool, args).await?;
                if let Some(s) = result_val.as_str() {
                    Ok(s.to_string())
                } else {
                    Ok(result_val.to_string())
                }
            }
            "skill" => {
                if let Ok(Some(entry)) = Self::get_tool(name) {
                    let tool_dir = std::path::PathBuf::from(&entry.local_dir);
                    let skill_file = tool_dir.join("SKILL.md");
                    if skill_file.exists() {
                        let content = std::fs::read_to_string(&skill_file)?;
                        return Ok(content);
                    }
                }
                Ok(format!("Skill '{}' active. Ready for workflow execution.", name))
            }
            "plugin" | _ => {
                let bytes = Self::execute_plugin_by_name(name, payload.as_bytes())?;
                Ok(String::from_utf8_lossy(&bytes).to_string())
            }
        }
    }
}

