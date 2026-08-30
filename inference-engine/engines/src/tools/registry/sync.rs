use anyhow::Result;
use cluaiz_shared::environment::EnvironmentManager;
use super::index::ToolsRegistry;
use super::types::{ExecutionMode, SecurityMode, ToolEntry};

impl ToolsRegistry {
    /// Auto-probes ~/.cluaiz/tools/skills, plugins, and mcp directories
    /// Synchronizes missing entries into the registry and purges deleted tools
    pub fn sync_with_filesystem(&mut self) -> Result<()> {
        let env = EnvironmentManager::current();
        let _ = env.ensure_tools_dir();
        let skills_dir = env.skills_dir();
        let plugins_dir = env.plugins_dir();
        let mcp_dir = env.mcp_dir();

        let mut changed = false;

        // 1. Purge deleted tools
        let current_ids: Vec<String> = self.installed_tools.keys().cloned().collect();
        for id in current_ids {
            if let Some(entry) = self.installed_tools.get(&id) {
                let path = std::path::PathBuf::from(&entry.local_dir);
                if !path.exists() {
                    self.installed_tools.remove(&id);
                    changed = true;
                    tracing::info!("🧹 [ToolsRegistry] Removed uninstalled tool: {}", id);
                }
            }
        }

        // 2. Scan Skills (~/.cluaiz/tools/skills/)
        if skills_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&skills_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let tool_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let (name, ver, desc, triggers, mode, turns) = Self::probe_skill_metadata(&path);
                        if let Some(existing) = self.installed_tools.get_mut(&tool_id) {
                            if !name.is_empty() { existing.name = name; }
                            if !ver.is_empty() { existing.version = ver; }
                            if !desc.is_empty() { existing.description = desc; }
                            existing.semantic_triggers = triggers;
                            existing.execution_mode = mode;
                            existing.default_turns = turns;
                            existing.local_dir = path.to_string_lossy().to_string();
                        } else {
                            self.installed_tools.insert(tool_id.clone(), ToolEntry {
                                id: tool_id.clone(),
                                name: if name.is_empty() { tool_id.clone() } else { name },
                                category: "skill".to_string(),
                                version: ver,
                                description: desc,
                                local_dir: path.to_string_lossy().to_string(),
                                binary_path: None,
                                enabled: true,
                                security_mode: SecurityMode::FullAccess,
                                execution_mode: mode,
                                default_turns: turns,
                                permissions: Vec::new(),
                                semantic_triggers: triggers,
                                activation_events: Vec::new(),
                            });
                            changed = true;
                            tracing::info!("✨ [ToolsRegistry] Discovered new Skill: {}", tool_id);
                        }
                    }
                }
            }
        }

        // 3. Scan Plugins (~/.cluaiz/tools/plugins/)
        if plugins_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&plugins_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let tool_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let (name, ver, desc, binary, triggers, perms, mode, turns) = Self::probe_plugin_metadata(&path);
                        if let Some(existing) = self.installed_tools.get_mut(&tool_id) {
                            if !name.is_empty() { existing.name = name; }
                            if !ver.is_empty() { existing.version = ver; }
                            if !desc.is_empty() { existing.description = desc; }
                            existing.binary_path = binary;
                            existing.semantic_triggers = triggers;
                            existing.permissions = perms;
                            existing.execution_mode = mode;
                            existing.default_turns = turns;
                            existing.local_dir = path.to_string_lossy().to_string();
                        } else {
                            self.installed_tools.insert(tool_id.clone(), ToolEntry {
                                id: tool_id.clone(),
                                name: if name.is_empty() { tool_id.clone() } else { name },
                                category: "plugin".to_string(),
                                version: ver,
                                description: desc,
                                local_dir: path.to_string_lossy().to_string(),
                                binary_path: binary,
                                enabled: true,
                                security_mode: SecurityMode::FullAccess,
                                execution_mode: mode,
                                default_turns: turns,
                                permissions: perms,
                                semantic_triggers: triggers,
                                activation_events: vec![format!("on_command:use plugin::{}", tool_id)],
                            });
                            changed = true;
                            tracing::info!("✨ [ToolsRegistry] Discovered new Plugin: {}", tool_id);
                        }
                    }
                }
            }
        }

        // 4. Scan MCP (~/.cluaiz/tools/mcp/)
        if mcp_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&mcp_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let tool_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let (name, ver, desc, triggers, perms, mode, turns) = Self::probe_mcp_metadata(&path);
                        if let Some(existing) = self.installed_tools.get_mut(&tool_id) {
                            if !name.is_empty() { existing.name = name; }
                            if !ver.is_empty() { existing.version = ver; }
                            if !desc.is_empty() { existing.description = desc; }
                            existing.semantic_triggers = triggers;
                            existing.permissions = perms;
                            existing.execution_mode = mode;
                            existing.default_turns = turns;
                            existing.local_dir = path.to_string_lossy().to_string();
                        } else {
                            self.installed_tools.insert(tool_id.clone(), ToolEntry {
                                id: tool_id.clone(),
                                name: if name.is_empty() { tool_id.clone() } else { name },
                                category: "mcp".to_string(),
                                version: ver,
                                description: desc,
                                local_dir: path.to_string_lossy().to_string(),
                                binary_path: None,
                                enabled: true,
                                security_mode: SecurityMode::Strict,
                                execution_mode: mode,
                                default_turns: turns,
                                permissions: perms,
                                semantic_triggers: triggers,
                                activation_events: vec![format!("on_command:use mcp::{}", tool_id)],
                            });
                            changed = true;
                            tracing::info!("✨ [ToolsRegistry] Discovered new MCP: {}", tool_id);
                        }
                    }
                }
            }
        }

        if changed {
            let _ = self.save();
        }
        Ok(())
    }

    fn parse_execution_mode(val: Option<&str>, default_mode: ExecutionMode) -> ExecutionMode {
        match val.unwrap_or("").to_lowercase().as_str() {
            "auto" => ExecutionMode::Auto,
            "manual" => ExecutionMode::Manual,
            _ => default_mode,
        }
    }

    fn probe_skill_metadata(dir: &std::path::Path) -> (String, String, String, Vec<String>, ExecutionMode, i32) {
        let skill_md = dir.join("SKILL.md");
        if skill_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&skill_md) {
                return Self::probe_skill_frontmatter(&content);
            }
        }
        (String::new(), String::new(), String::new(), Vec::new(), ExecutionMode::Auto, -1)
    }

    fn probe_skill_frontmatter(content: &str) -> (String, String, String, Vec<String>, ExecutionMode, i32) {
        let normalized = content.replace("\r\n", "\n");
        let mut start_idx = None;
        if normalized.starts_with("---\n") {
            start_idx = Some(0);
        } else if let Some(idx) = normalized.find("\n---\n") {
            start_idx = Some(idx + 1);
        }

        if let Some(start) = start_idx {
            let after_open = &normalized[start + 4..];
            if let Some(end) = after_open.find("\n---") {
                let yaml_str = &after_open[..end];
                if let Ok(val) = serde_yaml::from_str::<serde_json::Value>(yaml_str) {
                    let name = val.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let version = val.get("version").or_else(|| val.get("latest_version")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let desc = val.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                    
                    let mut triggers = Vec::new();
                    if let Some(t_arr) = val.get("triggers").and_then(|t| t.as_array()) {
                        triggers = t_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    } else if let Some(t_obj) = val.get("triggers").or_else(|| val.get("discovery")) {
                        if let Some(arr) = t_obj.get("semantic_triggers").or_else(|| t_obj.get("semantic")).and_then(|s| s.as_array()) {
                            triggers = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        }
                    }

                    let mode = Self::parse_execution_mode(val.get("execution_mode").and_then(|m| m.as_str()), ExecutionMode::Auto);
                    let default_turns = val.get("default_turns").and_then(|t| t.as_i64()).unwrap_or(-1) as i32;

                    return (name, version, desc, triggers, mode, default_turns);
                }
            }
        }
        (String::new(), String::new(), String::new(), Vec::new(), ExecutionMode::Auto, -1)
    }

    fn probe_plugin_metadata(dir: &std::path::Path) -> (String, String, String, Option<String>, Vec<String>, Vec<String>, ExecutionMode, i32) {
        let mut binary = None;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().map_or(false, |ext| ext == "wasm" || ext == "dll" || ext == "so" || ext == "dylib") {
                    binary = Some(p.to_string_lossy().to_string());
                    break;
                }
            }
        }

        let mut name = String::new();
        let mut version = String::new();
        let mut desc = String::new();
        let mut triggers = Vec::new();
        let mut perms = Vec::new();
        let mut mode = ExecutionMode::Auto;
        let mut default_turns = -1;

        let pkg_json = dir.join("package.json");
        if pkg_json.exists() {
            if let Ok(c) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&c) {
                    if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
                        name = id.to_string();
                    } else if let Some(n) = val.get("name").and_then(|v| v.as_str()) {
                        name = n.to_string();
                    }
                    if let Some(v) = val.get("latest_version").or_else(|| val.get("version")).and_then(|v| v.as_str()) {
                        version = v.to_string();
                    }
                    if let Some(d) = val.get("description").and_then(|v| v.as_str()) {
                        desc = d.to_string();
                    }
                    if let Some(d) = val.get("discovery") {
                        if let Some(arr) = d.get("semantic_triggers").and_then(|s| s.as_array()) {
                            triggers = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        }
                    } else if let Some(t_arr) = val.get("triggers").and_then(|t| t.as_array()) {
                        triggers = t_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }
                    if let Some(p) = val.get("permissions") {
                        if p.get("network_access").and_then(|n| n.as_bool()).unwrap_or(false) {
                            perms.push("net:fetch".to_string());
                        }
                        if p.get("file_system").and_then(|f| f.as_str()).unwrap_or("none") != "none" {
                            perms.push("fs:access".to_string());
                        }
                    }
                    mode = Self::parse_execution_mode(val.get("execution_mode").and_then(|m| m.as_str()), ExecutionMode::Auto);
                    default_turns = val.get("default_turns").and_then(|t| t.as_i64()).unwrap_or(-1) as i32;
                    return (name, version, desc, binary, triggers, perms, mode, default_turns);
                }
            }
        }

        let skill_path = dir.join("SKILL.md");
        if skill_path.exists() {
            if let Ok(s_content) = std::fs::read_to_string(&skill_path) {
                let (name, ver, desc, triggers, mode, turns) = Self::probe_skill_frontmatter(&s_content);
                return (name, ver, desc, binary, triggers, Vec::new(), mode, turns);
            }
        }

        (name, version, desc, binary, triggers, perms, mode, default_turns)
    }

    fn probe_mcp_metadata(dir: &std::path::Path) -> (String, String, String, Vec<String>, Vec<String>, ExecutionMode, i32) {
        let mut name = String::new();
        let mut version = String::new();
        let mut desc = String::new();
        let mut triggers = Vec::new();
        let mut perms = Vec::new();
        let mut mode = ExecutionMode::Manual;
        let mut default_turns = 3;

        let pkg_json = dir.join("package.json");
        if pkg_json.exists() {
            if let Ok(c) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&c) {
                    if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
                        name = id.to_string();
                    } else if let Some(n) = val.get("name").and_then(|v| v.as_str()) {
                        name = n.to_string();
                    }
                    if let Some(v) = val.get("latest_version").or_else(|| val.get("version")).and_then(|v| v.as_str()) {
                        version = v.to_string();
                    }
                    if let Some(d) = val.get("description").and_then(|v| v.as_str()) {
                        desc = d.to_string();
                    }
                    if let Some(d) = val.get("discovery") {
                        if let Some(arr) = d.get("semantic_triggers").and_then(|s| s.as_array()) {
                            triggers = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                        }
                    } else if let Some(t_arr) = val.get("triggers").and_then(|t| t.as_array()) {
                        triggers = t_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }
                    if let Some(p) = val.get("permissions") {
                        if p.get("network_access").and_then(|n| n.as_bool()).unwrap_or(false) {
                            perms.push("net:fetch".to_string());
                        }
                    }
                    mode = Self::parse_execution_mode(val.get("execution_mode").and_then(|m| m.as_str()), ExecutionMode::Manual);
                    default_turns = val.get("default_turns").and_then(|t| t.as_i64()).unwrap_or(3) as i32;
                    return (name, version, desc, triggers, perms, mode, default_turns);
                }
            }
        }
        (name, version, desc, triggers, perms, mode, default_turns)
    }
}
