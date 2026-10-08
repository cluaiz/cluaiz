use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use engine_core::environment::EnvironmentManager;
use crate::tools::lifecycle::SessionToolManager;
use crate::tools::mcp::McpClient;
use crate::tools::registry::ToolsRegistry;
use crate::tools::skills::SkillRouter;

/// Target resolution information for routing a function call emitted by an LLM
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedToolTarget {
    /// Tool category: "mcp", "plugin", or "skill"
    pub category: String,
    /// Root component ID (directory or registry key, e.g. "filesystem", "math")
    pub component_id: String,
    /// Sub-function or method name inside the component (e.g. "read_file", "evaluate")
    pub sub_function: Option<String>,
}

/// Output of compiling tools and skills for a specific chat prompt
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompiledPromptTools {
    /// Formatted `<tools> [ ... ] </tools>` block or empty string if no executable tools are active
    pub tools_xml: String,
    /// Joined system instruction guidelines from active skills
    pub skill_instructions: String,
    /// List of active tool IDs included in this compilation
    pub active_tool_ids: Vec<String>,
    /// Estimated token count for context accounting
    pub estimated_tokens: usize,
}


/// Dynamic compiler that aggregates active Skills, Plugins (WASM/Native), and MCP Tools
/// into standard ChatML / OpenAI JSON schemas for LLM prompts.
pub struct ToolPromptCompiler;

impl ToolPromptCompiler {
    /// Compiles prompt tools and instructions for an active session ID
    pub async fn compile_for_session(session_id: &str) -> CompiledPromptTools {
        let session_tools = SessionToolManager::get_session_tools(session_id);
        let tool_ids: Vec<String> = session_tools.into_iter().map(|b| b.id).collect();
        Self::compile_tools(&tool_ids).await
    }

    /// Compiles prompt tools and instructions for a specific list of tool IDs
    pub async fn compile_tools(tool_ids: &[String]) -> CompiledPromptTools {
        if tool_ids.is_empty() {
            return CompiledPromptTools::default();
        }

        let registry = ToolsRegistry::load().ok();
        let env = EnvironmentManager::current();
        let skill_router = SkillRouter::new();

        let mut skill_instructions_vec = Vec::new();
        let mut function_schemas: Vec<Value> = Vec::new();
        let mut active_tool_ids = Vec::new();
        let mut allowed_tools_whitelist: Option<std::collections::HashSet<String>> = None;

        for tool_id in tool_ids {
            let normalized_id = tool_id.trim();
            if normalized_id.is_empty() {
                continue;
            }

            // 1. Resolve metadata from registry or filesystem
            let (category, local_dir, name, description) = if let Some(ref reg) = registry {
                if let Some(entry) = reg.get_tool(normalized_id) {
                    (
                        entry.category.clone(),
                        PathBuf::from(&entry.local_dir),
                        entry.name.clone(),
                        entry.description.clone(),
                    )
                } else {
                    Self::resolve_filesystem_tool(&env, normalized_id)
                }
            } else {
                Self::resolve_filesystem_tool(&env, normalized_id)
            };

            active_tool_ids.push(normalized_id.to_string());

            match category.as_str() {
                "skill" => {
                    // Skill: Pure Markdown Prompt Instruction
                    if let Some(instructions) = skill_router.get_instructions(normalized_id) {
                        skill_instructions_vec.push(instructions.to_string());
                    } else {
                        let skill_file = local_dir.join("SKILL.md");
                        if let Ok(content) = std::fs::read_to_string(&skill_file) {
                            if let Some(parsed) = crate::tools::skills::SkillParser::parse_content(&content) {
                                skill_instructions_vec.push(parsed.prompt_instructions);
                            }
                        }
                    }

                    if let Some(allowed) = skill_router.get_allowed_tools(normalized_id) {
                        let mut set = allowed_tools_whitelist.unwrap_or_default();
                        for tool in allowed {
                            set.insert(tool.to_lowercase().trim().to_string());
                        }
                        allowed_tools_whitelist = Some(set);
                    } else {
                        let skill_file = local_dir.join("SKILL.md");
                        if let Ok(content) = std::fs::read_to_string(&skill_file) {
                            if let Some(parsed) = crate::tools::skills::SkillParser::parse_content(&content) {
                                if !parsed.metadata.allowed_tools.is_empty() {
                                    let mut set = allowed_tools_whitelist.unwrap_or_default();
                                    for tool in &parsed.metadata.allowed_tools {
                                        set.insert(tool.to_lowercase().trim().to_string());
                                    }
                                    allowed_tools_whitelist = Some(set);
                                }
                            }
                        }
                    }

                    // Check if skill provides executable function tools in package.json
                    let pkg_json = local_dir.join("package.json");
                    if pkg_json.exists() {
                        let schemas = Self::extract_plugin_schemas(&local_dir, normalized_id, &name, &description);
                        function_schemas.extend(schemas);
                    }
                }
                "plugin" => {
                    // WASM / Native Plugin: JSON schema functions
                    let schemas = Self::extract_plugin_schemas(&local_dir, normalized_id, &name, &description);
                    function_schemas.extend(schemas);
                }
                "mcp" => {
                    // MCP Server: Dynamic discovery over stdio JSON-RPC (tools/list)
                    let schemas = Self::extract_mcp_schemas(&local_dir, normalized_id, &name, &description).await;
                    function_schemas.extend(schemas);
                }
                _ => {
                    // Fallback: Check if it's a known skill or plugin by directory presence
                    if local_dir.join("SKILL.md").exists() {
                        if let Ok(content) = std::fs::read_to_string(local_dir.join("SKILL.md")) {
                            if let Some(parsed) = crate::tools::skills::SkillParser::parse_content(&content) {
                                skill_instructions_vec.push(parsed.prompt_instructions);
                            }
                        }
                        let pkg_json = local_dir.join("package.json");
                        if pkg_json.exists() {
                            let schemas = Self::extract_plugin_schemas(&local_dir, normalized_id, &name, &description);
                            function_schemas.extend(schemas);
                        }
                    } else if local_dir.join("package.json").exists() {
                        let schemas = Self::extract_plugin_schemas(&local_dir, normalized_id, &name, &description);
                        function_schemas.extend(schemas);
                    }
                }
            }
        }

        // Filter function schemas if an active skill restricts allowed-tools
        if let Some(ref whitelist) = allowed_tools_whitelist {
            function_schemas.retain(|schema| {
                let tool_fn_name = schema.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .or_else(|| schema.get("name").and_then(|n| n.as_str()));

                if let Some(name) = tool_fn_name {
                    let name_lower = name.to_lowercase();
                    whitelist.contains(&name_lower)
                        || whitelist.iter().any(|allowed| name_lower.ends_with(&format!("__{}", allowed)) || name_lower.contains(allowed))
                } else {
                    true
                }
            });
        }

        // Format standard `<tools> [ ... ] </tools>` container
        let tools_xml = if !function_schemas.is_empty() {
            let json_formatted = serde_json::to_string_pretty(&function_schemas).unwrap_or_else(|_| "[]".to_string());
            format!("<tools>\n{}\n</tools>", json_formatted)
        } else {
            String::new()
        };

        let skill_instructions = skill_instructions_vec.join("\n\n---\n\n");
        let raw_char_len = tools_xml.len() + skill_instructions.len();
        let estimated_tokens = (raw_char_len + 3) / 4;

        CompiledPromptTools {
            tools_xml,
            skill_instructions,
            active_tool_ids,
            estimated_tokens,
        }
    }

    /// Resolves tool category and location directly from standard directories when not found in registry
    fn resolve_filesystem_tool(env: &EnvironmentManager, tool_id: &str) -> (String, PathBuf, String, String) {
        let skill_path = env.skills_dir().join(tool_id);
        if skill_path.exists() {
            return ("skill".to_string(), skill_path, tool_id.to_string(), String::new());
        }

        let plugin_path = env.plugins_dir().join(tool_id);
        if plugin_path.exists() {
            return ("plugin".to_string(), plugin_path, tool_id.to_string(), String::new());
        }

        let mcp_path = env.mcp_dir().join(tool_id);
        if mcp_path.exists() {
            return ("mcp".to_string(), mcp_path, tool_id.to_string(), String::new());
        }

        // Check global fallback directory
        let global_plugin = env.global_dir.join("plugins").join(tool_id);
        if global_plugin.exists() {
            return ("plugin".to_string(), global_plugin, tool_id.to_string(), String::new());
        }

        ("unknown".to_string(), PathBuf::from(tool_id), tool_id.to_string(), String::new())
    }

    /// Extracts function schemas for a WASM/Native plugin from its `package.json` manifest
    fn extract_plugin_schemas(local_dir: &Path, plugin_id: &str, name: &str, fallback_desc: &str) -> Vec<Value> {
        let mut schemas = Vec::new();
        let manifest_path = local_dir.join("package.json");

        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                // Check if `functions` array exists (Standard OpenAI/Cluaiz format)
                if let Some(funcs) = val.get("functions").and_then(|f| f.as_array()) {
                    for f in funcs {
                        let fn_name = f.get("name").and_then(|n| n.as_str()).unwrap_or(plugin_id);
                        let fn_desc = f.get("description").and_then(|d| d.as_str()).unwrap_or(fallback_desc);
                        let fn_params = f.get("parameters").cloned().unwrap_or_else(|| {
                            json!({
                                "type": "object",
                                "properties": {}
                            })
                        });

                        schemas.push(json!({
                            "type": "function",
                            "function": {
                                "name": fn_name,
                                "description": fn_desc,
                                "parameters": fn_params
                            }
                        }));
                    }
                } else if let Some(tools) = val.get("tools").and_then(|t| t.as_array()) {
                    // Check if `tools` array exists
                    for t in tools {
                        let tool_name = t.get("name").and_then(|n| n.as_str()).unwrap_or(plugin_id);
                        let tool_desc = t.get("description").and_then(|d| d.as_str()).unwrap_or(fallback_desc);
                        let tool_params = t.get("parameters").or_else(|| t.get("inputSchema")).cloned().unwrap_or_else(|| {
                            json!({
                                "type": "object",
                                "properties": {}
                            })
                        });

                        schemas.push(json!({
                            "type": "function",
                            "function": {
                                "name": tool_name,
                                "description": tool_desc,
                                "parameters": tool_params
                            }
                        }));
                    }
                }
            }
        }

        // If no explicit function array was defined, synthesize a default function schema
        if schemas.is_empty() {
            let desc = if !fallback_desc.is_empty() {
                fallback_desc.to_string()
            } else if !name.is_empty() {
                format!("Execute {} plugin operation", name)
            } else {
                format!("Execute {} tool operation", plugin_id)
            };

            // Heuristic for math plugins: express input as expression
            let properties = if plugin_id.contains("math") || plugin_id.contains("calc") {
                json!({
                    "expression": {
                        "type": "string",
                        "description": "Mathematical formula to evaluate (e.g. 'sqrt(256) * 14.5')"
                    },
                    "precision": {
                        "type": "integer",
                        "description": "Decimal precision places (default: 4)"
                    }
                })
            } else if plugin_id.contains("search") {
                json!({
                    "query": {
                        "type": "string",
                        "description": "Search query or target URL"
                    }
                })
            } else {
                json!({
                    "input": {
                        "type": "string",
                        "description": "Input argument string or JSON string to execute"
                    }
                })
            };

            let required = if plugin_id.contains("math") || plugin_id.contains("calc") {
                vec!["expression"]
            } else if plugin_id.contains("search") {
                vec!["query"]
            } else {
                vec!["input"]
            };

            schemas.push(json!({
                "type": "function",
                "function": {
                    "name": plugin_id,
                    "description": desc,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                }
            }));
        }

        schemas
    }

    /// Extracts function schemas from an external MCP server via stdio dynamic discovery (`tools/list`)
    async fn extract_mcp_schemas(local_dir: &Path, mcp_id: &str, fallback_name: &str, fallback_desc: &str) -> Vec<Value> {
        let mut schemas = Vec::new();

        // 1. Attempt dynamic discovery via McpClient::list_tools
        match McpClient::list_tools(local_dir).await {
            Ok(tools_vec) => {
                for tool in tools_vec {
                    if let Some(tool_name) = tool.get("name").and_then(|n| n.as_str()) {
                        let desc = tool.get("description").and_then(|d| d.as_str()).unwrap_or(fallback_desc);
                        let input_schema = tool.get("inputSchema").cloned().unwrap_or_else(|| {
                            json!({
                                "type": "object",
                                "properties": {}
                            })
                        });

                        schemas.push(json!({
                            "type": "function",
                            "function": {
                                "name": tool_name,
                                "description": desc,
                                "parameters": input_schema
                            }
                        }));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("⚠️ [ToolPromptCompiler] Dynamic MCP discovery failed for '{}': {}", mcp_id, e);
            }
        }

        // 2. Fallback: If dynamic discovery yielded nothing, check static manifest
        if schemas.is_empty() {
            let manifest_path = local_dir.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(val) = serde_json::from_str::<Value>(&content) {
                    if let Some(tools_arr) = val.get("tools").and_then(|t| t.as_array()) {
                        for t in tools_arr {
                            if let Some(tool_name) = t.get("name").and_then(|n| n.as_str()) {
                                let desc = t.get("description").and_then(|d| d.as_str()).unwrap_or(fallback_desc);
                                let input_schema = t.get("inputSchema").or_else(|| t.get("parameters")).cloned().unwrap_or_else(|| {
                                    json!({
                                        "type": "object",
                                        "properties": {}
                                    })
                                });

                                schemas.push(json!({
                                    "type": "function",
                                    "function": {
                                        "name": tool_name,
                                        "description": desc,
                                        "parameters": input_schema
                                    }
                                }));
                            }
                        }
                    }
                }
            }
        }

        // 3. Last-resort fallback if no individual methods could be discovered
        if schemas.is_empty() {
            let desc = if !fallback_desc.is_empty() {
                fallback_desc.to_string()
            } else {
                format!("Model Context Protocol tool bridge for {}", fallback_name)
            };

            schemas.push(json!({
                "type": "function",
                "function": {
                    "name": mcp_id,
                    "description": desc,
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {
                                "type": "string",
                                "description": "MCP operation or action to execute"
                            },
                            "arguments": {
                                "type": "object",
                                "description": "Arguments payload dictionary"
                            }
                        },
                        "required": ["command"]
                    }
                }
            }));
        }

        schemas
    }

    /// Resolves an emitted function name to its host component (Plugin / MCP Server / Skill)
    pub fn resolve_target(fn_name: &str, candidate_tool_ids: &[String]) -> Option<ResolvedToolTarget> {
        let trimmed_name = fn_name.trim();
        if trimmed_name.is_empty() {
            return None;
        }

        // 1. Explicit colon-separated syntax (e.g. "plugin:math", "mcp:filesystem:read_file", "filesystem:read_file")
        if trimmed_name.contains(':') {
            let parts: Vec<&str> = trimmed_name.split(':').collect();
            if parts.len() == 2 {
                if parts[0] == "plugin" || parts[0] == "mcp" || parts[0] == "skill" {
                    return Some(ResolvedToolTarget {
                        category: parts[0].to_string(),
                        component_id: parts[1].to_string(),
                        sub_function: None,
                    });
                } else {
                    return Some(ResolvedToolTarget {
                        category: "mcp".to_string(),
                        component_id: parts[0].to_string(),
                        sub_function: Some(parts[1].to_string()),
                    });
                }
            } else if parts.len() >= 3 {
                return Some(ResolvedToolTarget {
                    category: parts[0].to_string(),
                    component_id: parts[1].to_string(),
                    sub_function: Some(parts[2].to_string()),
                });
            }
        }

        let env = EnvironmentManager::current();
        let registry = ToolsRegistry::load().ok();

        // 2. Direct component name match against candidate IDs or Registry
        for candidate in candidate_tool_ids {
            if candidate.eq_ignore_ascii_case(trimmed_name) {
                let (category, _, _, _) = Self::resolve_filesystem_tool(&env, candidate);
                return Some(ResolvedToolTarget {
                    category,
                    component_id: candidate.clone(),
                    sub_function: None,
                });
            }
        }

        if let Some(ref reg) = registry {
            if let Some(entry) = reg.get_tool(trimmed_name) {
                return Some(ResolvedToolTarget {
                    category: entry.category.clone(),
                    component_id: entry.id.clone(),
                    sub_function: None,
                });
            }
        }

        // Direct directory match in plugins, mcp, or skills
        let (cat, _, comp_id, _) = Self::resolve_filesystem_tool(&env, trimmed_name);
        if cat != "unknown" {
            return Some(ResolvedToolTarget {
                category: cat,
                component_id: comp_id,
                sub_function: None,
            });
        }

        // 3. Sub-function reverse resolution: Search across candidates (and all registry tools)
        let mut search_pool: Vec<String> = candidate_tool_ids.iter().cloned().collect();
        if let Some(ref reg) = registry {
            for id in reg.installed_tools.keys() {
                if !search_pool.contains(id) {
                    search_pool.push(id.clone());
                }
            }
        }

        for comp_id in &search_pool {
            let (category, local_dir, _, _) = if let Some(ref reg) = registry {
                if let Some(entry) = reg.get_tool(comp_id) {
                    (entry.category.clone(), PathBuf::from(&entry.local_dir), entry.name.clone(), entry.description.clone())
                } else {
                    Self::resolve_filesystem_tool(&env, comp_id)
                }
            } else {
                Self::resolve_filesystem_tool(&env, comp_id)
            };

            if category == "mcp" {
                // Check cached MCP tools or package.json
                if let Some(tools) = McpClient::get_cached_tools(&local_dir) {
                    for tool in tools {
                        if let Some(name) = tool.get("name").and_then(|n| n.as_str()) {
                            if name.eq_ignore_ascii_case(trimmed_name) {
                                return Some(ResolvedToolTarget {
                                    category: "mcp".to_string(),
                                    component_id: comp_id.clone(),
                                    sub_function: Some(trimmed_name.to_string()),
                                });
                            }
                        }
                    }
                }
            } else if category == "plugin" || category == "skill" {
                // Check plugin or skill package.json
                let manifest_path = local_dir.join("package.json");
                if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                        if let Some(funcs) = val.get("functions").and_then(|f| f.as_array()) {
                            for f in funcs {
                                if let Some(name) = f.get("name").and_then(|n| n.as_str()) {
                                    if name.eq_ignore_ascii_case(trimmed_name) {
                                        return Some(ResolvedToolTarget {
                                            category: category.clone(),
                                            component_id: comp_id.clone(),
                                            sub_function: Some(trimmed_name.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                        if let Some(tools) = val.get("tools").and_then(|t| t.as_array()) {
                            for t in tools {
                                if let Some(name) = t.get("name").and_then(|n| n.as_str()) {
                                    if name.eq_ignore_ascii_case(trimmed_name) {
                                        return Some(ResolvedToolTarget {
                                            category: category.clone(),
                                            component_id: comp_id.clone(),
                                            sub_function: Some(trimmed_name.to_string()),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Well-known tool name heuristics
        if trimmed_name.starts_with("math_") || trimmed_name == "calculate" || trimmed_name == "evaluate_expression" {
            let math_dir = env.plugins_dir().join("math");
            if math_dir.exists() {
                return Some(ResolvedToolTarget {
                    category: "plugin".to_string(),
                    component_id: "math".to_string(),
                    sub_function: Some(trimmed_name.to_string()),
                });
            }
        }
        if trimmed_name == "read_file" || trimmed_name == "write_file" || trimmed_name == "list_directory" || trimmed_name == "directory_tree" {
            let fs_dir = env.mcp_dir().join("filesystem");
            if fs_dir.exists() {
                return Some(ResolvedToolTarget {
                    category: "mcp".to_string(),
                    component_id: "filesystem".to_string(),
                    sub_function: Some(trimmed_name.to_string()),
                });
            }
        }
        if trimmed_name == "web_search" || trimmed_name == "search" {
            let ws_dir = env.plugins_dir().join("web-search");
            if ws_dir.exists() {
                return Some(ResolvedToolTarget {
                    category: "plugin".to_string(),
                    component_id: "web-search".to_string(),
                    sub_function: Some(trimmed_name.to_string()),
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_tools_compilation() {
        let compiled = ToolPromptCompiler::compile_tools(&[]).await;
        assert_eq!(compiled.tools_xml, "");
        assert_eq!(compiled.skill_instructions, "");
        assert_eq!(compiled.estimated_tokens, 0);
    }

    #[tokio::test]
    async fn test_plugin_schema_synthesis() {
        let temp_dir = std::env::temp_dir().join("cluaiz_test_plugin_math");
        let _ = std::fs::create_dir_all(&temp_dir);

        let pkg_json = json!({
            "name": "math",
            "functions": [
                {
                    "name": "math_calculate",
                    "description": "Calculates math expression",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "expression": { "type": "string" }
                        },
                        "required": ["expression"]
                    }
                }
            ]
        });
        let _ = std::fs::write(temp_dir.join("package.json"), pkg_json.to_string());

        let schemas = ToolPromptCompiler::extract_plugin_schemas(&temp_dir, "math", "Math Evaluator", "Math description");
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["function"]["name"], "math_calculate");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_resolve_target_syntax() {
        let namespaced = ToolPromptCompiler::resolve_target("mcp:filesystem:read_file", &[]);
        assert_eq!(namespaced, Some(ResolvedToolTarget {
            category: "mcp".to_string(),
            component_id: "filesystem".to_string(),
            sub_function: Some("read_file".to_string()),
        }));

        let plugin_namespaced = ToolPromptCompiler::resolve_target("plugin:math", &[]);
        assert_eq!(plugin_namespaced, Some(ResolvedToolTarget {
            category: "plugin".to_string(),
            component_id: "math".to_string(),
            sub_function: None,
        }));
    }
}

