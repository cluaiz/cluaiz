pub mod installer;
pub mod lifecycle;
pub mod mcp;
pub mod plugins;
pub mod registry;
pub mod skills;
pub mod telemetry;

use std::path::Path;
use anyhow::Result;
use serde_json::Value;

pub use installer::ToolHubInstaller;
pub use lifecycle::{SessionToolBinding, SessionToolManager, TurnLifecycleEngine};
pub use mcp::{McpClient, McpManifest};
pub use plugins::{PluginExecutor, PluginManifest};
pub use registry::{ExecutionMode, LoadStrategy, ToolCategory, ToolEntry, ToolsRegistry};
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

    /// Downloads and installs a tool from Cluaiz Hub into `~/.cluaiz/tools/{skills,plugins,mcp}`
    pub async fn install_tool(category: &str, tool_id: &str) -> Result<()> {
        ToolHubInstaller::install_component(category, tool_id).await
    }

    /// Removes an installed tool from filesystem and syncs registry
    pub async fn remove_tool(category: &str, tool_id: &str) -> Result<()> {
        ToolHubInstaller::remove_component(category, tool_id).await
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
    pub fn compute_telemetry(session_id: &str, active_tool_ids: &[String], prompt_len: usize, history_len: usize) -> SystemContextTelemetry {
        ContextTracker::compute_telemetry(session_id, active_tool_ids, prompt_len, history_len)
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
        let env = cluaiz_shared::environment::EnvironmentManager::current();
        let plugin_dir = env.plugins_dir().join(plugin_name);
        if plugin_dir.exists() {
            Self::execute_plugin(&plugin_dir, payload)
        } else {
            let alt_dir = env.global_dir.join("plugins").join(plugin_name);
            if alt_dir.exists() {
                Self::execute_plugin(&alt_dir, payload)
            } else {
                Err(anyhow::anyhow!("Plugin '{}' not found in {:?}", plugin_name, plugin_dir))
            }
        }
    }

    /// Calls an external MCP tool via subprocess IPC
    pub async fn call_mcp(mcp_dir: &Path, tool_name: &str, arguments: Value) -> Result<Value> {
        McpClient::call_tool(mcp_dir, tool_name, arguments).await
    }
}
