//! 🎮 Engine Control API (Evolution of CEL to Typed JSON Control Operations)
//! Implements the 3-Tier Security Gate: Bearer Auth, Filesystem Jail, and Approval Gate.
//! Replaces legacy hand-rolled text lexer with standard JSON schema compatible with all models.

use anyhow::{anyhow, Result};
use cluaiz_security::guard::{PermissionGuard, PermissionLevel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", content = "args")]
pub enum ControlOp {
    /// Clears KV cache for a given session / sequence
    #[serde(rename = "kv.clear")]
    KvClear { session: String },

    /// Saves a named KV cache snapshot to disk
    #[serde(rename = "kv.save")]
    KvSave { session: String, name: String },

    /// Restores a named KV cache snapshot from disk
    #[serde(rename = "kv.restore")]
    KvRestore { session: String, name: String },

    /// Hierarchical memory search across past turns / messages
    #[serde(rename = "memory.search")]
    MemorySearch {
        query: String,
        scope: Option<String>,
        limit: Option<usize>,
    },

    /// Invokes a plugin / tool function with typed JSON arguments
    #[serde(rename = "plugin.call")]
    PluginCall {
        plugin: String,
        #[serde(rename = "fn")]
        function: String,
        input: serde_json::Value,
    },

    /// Standard Model Context Protocol (MCP) tool invocation
    #[serde(rename = "mcp.call")]
    McpCall {
        server: String,
        tool: String,
        input: serde_json::Value,
    },

    /// Loads instructions / skill into active context
    #[serde(rename = "skill.load")]
    SkillLoad { name: String },

    /// Sandboxed OS process execution with mandatory approval gate
    #[serde(rename = "os.exec")]
    OsExec { cmd: String, args: Vec<String> },

    /// Sets runtime configuration parameter
    #[serde(rename = "config.set")]
    ConfigSet {
        target: String,
        key: String,
        value: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlResult {
    pub success: bool,
    pub op: String,
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct ControlDispatcher {
    guard: PermissionGuard,
}

impl Default for ControlDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlDispatcher {
    pub fn new() -> Self {
        Self {
            guard: PermissionGuard::new(),
        }
    }

    /// Dispatches a single control operation, enforcing security gates
    pub async fn dispatch(&self, op: ControlOp, approved_by_user: bool) -> Result<ControlResult> {
        match op {
            ControlOp::KvClear { ref session } => {
                tracing::warn!("⚠️ [Control] kv.clear requested for session '{}', but KV backend binding is not implemented in agent runtime.", session);
                Ok(ControlResult {
                    success: false,
                    op: "kv.clear".into(),
                    data: serde_json::Value::Null,
                    error: Some(format!("KV cache backend binding is not implemented in agent runtime for session '{}'", session)),
                })
            }
            ControlOp::KvSave {
                ref session,
                ref name,
            } => {
                tracing::warn!("⚠️ [Control] kv.save requested for checkpoint '{}' (session: {}), not implemented.", name, session);
                Ok(ControlResult {
                    success: false,
                    op: "kv.save".into(),
                    data: serde_json::Value::Null,
                    error: Some(format!("KV checkpointing is not implemented in agent runtime for '{}'", name)),
                })
            }
            ControlOp::KvRestore {
                ref session,
                ref name,
            } => {
                tracing::warn!("⚠️ [Control] kv.restore requested for checkpoint '{}' (session: {}), not implemented.", name, session);
                Ok(ControlResult {
                    success: false,
                    op: "kv.restore".into(),
                    data: serde_json::Value::Null,
                    error: Some(format!("KV restoration is not implemented in agent runtime for '{}'", name)),
                })
            }
            ControlOp::MemorySearch {
                ref query,
                ref scope,
                limit,
            } => {
                tracing::warn!("⚠️ [Control] memory.search requested for '{}' in {:?}, not implemented.", query, scope);
                Ok(ControlResult {
                    success: false,
                    op: "memory.search".into(),
                    data: serde_json::Value::Null,
                    error: Some(format!("FTS5 hierarchical memory search is not implemented yet for query '{}'", query)),
                })
            }
            ControlOp::SkillLoad { ref name } => {
                tracing::info!("📖 [Control] Loading skill prompt: {}", name);
                self.guard.validate_action(name, PermissionLevel::ReadOnly)?;
                
                let reg = crate::tools::registry::ToolsRegistry::load();
                match reg {
                    Ok(r) => {
                        if let Some(entry) = r.installed_tools.get(name) {
                            let path = std::path::PathBuf::from(&entry.local_dir).join("SKILL.md");
                            if path.exists() {
                                let content = std::fs::read_to_string(&path).unwrap_or_default();
                                Ok(ControlResult {
                                    success: true,
                                    op: "skill.load".into(),
                                    data: serde_json::json!({ "skill": name, "instructions": content }),
                                    error: None,
                                })
                            } else {
                                Ok(ControlResult {
                                    success: false,
                                    op: "skill.load".into(),
                                    data: serde_json::Value::Null,
                                    error: Some(format!("SKILL.md not found at {}", path.display())),
                                })
                            }
                        } else {
                            Ok(ControlResult {
                                success: false,
                                op: "skill.load".into(),
                                data: serde_json::Value::Null,
                                error: Some(format!("Skill '{}' not found in ToolsRegistry", name)),
                            })
                        }
                    }
                    Err(e) => Ok(ControlResult {
                        success: false,
                        op: "skill.load".into(),
                        data: serde_json::Value::Null,
                        error: Some(format!("Failed to load ToolsRegistry: {}", e)),
                    }),
                }
            }
            ControlOp::McpCall {
                ref server,
                ref tool,
                ref input,
            } => {
                tracing::info!("🔌 [Control] Executing MCP tool: {}/{}", server, tool);
                let input_str = input.to_string();
                match crate::ToolsEngine::execute_tool_by_name("mcp", server, Some(tool), &input_str).await {
                    Ok(res_str) => {
                        let parsed: serde_json::Value = serde_json::from_str(&res_str).unwrap_or(serde_json::json!(res_str));
                        Ok(ControlResult {
                            success: true,
                            op: "mcp.call".into(),
                            data: serde_json::json!({ "server": server, "tool": tool, "result": parsed }),
                            error: None,
                        })
                    }
                    Err(e) => Ok(ControlResult {
                        success: false,
                        op: "mcp.call".into(),
                        data: serde_json::Value::Null,
                        error: Some(e.to_string()),
                    }),
                }
            }
            ControlOp::PluginCall {
                ref plugin,
                ref function,
                ref input,
            } => {
                tracing::info!("🧩 [Control] Executing plugin: {}/{}", plugin, function);
                let input_str = input.to_string();
                match crate::ToolsEngine::execute_tool_by_name("plugin", plugin, Some(function), &input_str).await {
                    Ok(res_str) => {
                        let parsed: serde_json::Value = serde_json::from_str(&res_str).unwrap_or(serde_json::json!(res_str));
                        Ok(ControlResult {
                            success: true,
                            op: "plugin.call".into(),
                            data: serde_json::json!({ "plugin": plugin, "function": function, "result": parsed }),
                            error: None,
                        })
                    }
                    Err(e) => Ok(ControlResult {
                        success: false,
                        op: "plugin.call".into(),
                        data: serde_json::Value::Null,
                        error: Some(e.to_string()),
                    }),
                }
            }
            ControlOp::OsExec { ref cmd, ref args } => {
                // 🛡️ GATE 3: Mandatory Human-In-The-Loop Approval Gate
                if !approved_by_user {
                    return Err(anyhow!(
                        "Security Gate: 'os.exec' requires explicit user interactive approval."
                    ));
                }
                self.guard
                    .validate_action(cmd, PermissionLevel::DangerFullAccess)?;
                tracing::warn!("⚡ [Control] Executing approved OS process: {} {:?}", cmd, args);
                
                let env_cwd = cluaiz_core::EnvironmentManager::current().local_dir.clone();
                match crate::execution::terminal_runner::SandboxTerminalRunner::execute_direct(
                    cmd,
                    args,
                    &env_cwd,
                    crate::tools::registry::SecurityMode::RequireApproval,
                ).await {
                    Ok(res) => {
                        let is_success = res.exit_code == 0;
                        Ok(ControlResult {
                            success: is_success,
                            op: "os.exec".into(),
                            data: serde_json::json!({
                                "cmd": cmd,
                                "args": args,
                                "exit_code": res.exit_code,
                                "stdout": res.stdout,
                                "stderr": res.stderr,
                                "duration_ms": res.duration_ms,
                            }),
                            error: if is_success { None } else { Some(format!("Process exited with code {}", res.exit_code)) },
                        })
                    }
                    Err(e) => Ok(ControlResult {
                        success: false,
                        op: "os.exec".into(),
                        data: serde_json::Value::Null,
                        error: Some(e.to_string()),
                    }),
                }
            }
            ControlOp::ConfigSet {
                ref target,
                ref key,
                ref value,
            } => {
                tracing::warn!("⚙️ [Control] Dynamic config mutation for {}.{} is not implemented yet.", target, key);
                Ok(ControlResult {
                    success: false,
                    op: "config.set".into(),
                    data: serde_json::Value::Null,
                    error: Some(format!("Dynamic config mutation for {}.{} is not implemented yet", target, key)),
                })
            }
        }
    }
}
