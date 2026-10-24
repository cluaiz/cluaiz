use super::types::{ExecutionMode, SecurityMode, ToolEntry};
use anyhow::Result;
use cluaiz_core::EnvironmentManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// The registry as last read, kept in RAM so a chat turn does not read and parse the tool
/// folders again on every lookup
struct Snapshot {
    registry: ToolsRegistry,
    /// Sizes and change times of the registry file and every tool's manifest files
    fingerprint: u64,
    /// When the fingerprint was last compared with the disk
    checked: Instant,
}

static SNAPSHOT: LazyLock<Mutex<Option<Snapshot>>> = LazyLock::new(|| Mutex::new(None));
/// Increases whenever the snapshot is replaced, so caches built from it know to rebuild
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How often the disk is checked for manual edits (installs and setting changes through the
/// Engine replace the snapshot at once)
const RECHECK: Duration = Duration::from_secs(2);

fn file_stamp(path: &Path, hasher: &mut impl Hasher) {
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(hasher);
        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH).hash(hasher);
    }
}

/// A cheap hash of what the registry is built from: the registry file and, for every
/// installed tool folder, its manifest files (metadata only, no reading)
fn disk_fingerprint() -> u64 {
    let env = EnvironmentManager::current();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    file_stamp(&env.tools_registry_json_path(), &mut hasher);
    for dir in [env.skills_dir(), env.plugins_dir(), env.mcp_dir()] {
        let mut folders: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        folders.sort();
        for folder in folders {
            folder.hash(&mut hasher);
            for file in ["SKILL.md", "package.json", ".cache/tools_list.json"] {
                file_stamp(&folder.join(file), &mut hasher);
            }
        }
    }
    hasher.finish()
}

fn store_snapshot(registry: &ToolsRegistry) {
    let fingerprint = disk_fingerprint();
    *SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()) =
        Some(Snapshot { registry: registry.clone(), fingerprint, checked: Instant::now() });
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Master Tools Registry (Single Source of Truth for all Skills, Plugins, and MCP connectors)
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct ToolsRegistry {
    #[serde(default)]
    pub version: String,

    #[serde(default)]
    pub last_updated: String,

    /// Map of tool_id -> ToolEntry
    #[serde(default)]
    pub installed_tools: HashMap<String, ToolEntry>,
}

impl ToolsRegistry {
    /// Returns canonical path to `tools_registry.json` (~/.cluaiz/engine/config/tools_registry.json)
    pub fn registry_path() -> PathBuf {
        EnvironmentManager::current().tools_registry_json_path()
    }

    /// Changes whenever the registry in RAM is replaced (an install, a setting change, a manual
    /// edit on disk). Caches derived from the registry rebuild when it changes.
    pub fn generation() -> u64 {
        let _ = Self::load();
        GENERATION.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The tools registry. It is kept in RAM and read from `tools_registry.json` (plus the tool
    /// folders) only when something there changed; changes made through the Engine replace it
    /// at once, manual edits are seen within a few seconds.
    pub fn load() -> Result<Self> {
        {
            let mut guard = SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(snapshot) = guard.as_mut() {
                if snapshot.checked.elapsed() < RECHECK {
                    return Ok(snapshot.registry.clone());
                }
                if disk_fingerprint() == snapshot.fingerprint {
                    snapshot.checked = Instant::now();
                    return Ok(snapshot.registry.clone());
                }
            }
        }
        let registry = Self::load_from_disk()?;
        store_snapshot(&registry);
        Ok(registry)
    }

    /// Reads `tools_registry.json` (or migrates the legacy YAML, or builds it from the tool
    /// folders) and brings it in line with the installed folders
    fn load_from_disk() -> Result<Self> {
        let json_path = Self::registry_path();
        let env = EnvironmentManager::current();

        if json_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&json_path) {
                if let Ok(mut reg) = serde_json::from_str::<ToolsRegistry>(&content) {
                    let _ = reg.sync_with_filesystem();
                    return Ok(reg);
                }
            }
        }

        // 3. Fallback: Legacy migration from registry.yaml
        let legacy_yaml_path = env.config_dir().join("registry.yaml");
        if legacy_yaml_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&legacy_yaml_path) {
                if let Ok(legacy_val) = serde_yaml::from_str::<serde_json::Value>(&content) {
                    let mut reg = ToolsRegistry::default();
                    // Migrate plugins
                    if let Some(plugins) = legacy_val.get("plugins").and_then(|p| p.as_object()) {
                        for (k, v) in plugins {
                            reg.installed_tools.insert(
                                k.clone(),
                                ToolEntry {
                                    id: k.clone(),
                                    name: v
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or(k)
                                        .to_string(),
                                    category: "plugin".to_string(),
                                    version: v
                                        .get("version")
                                        .and_then(|ver| ver.as_str())
                                        .unwrap_or("1.0.0")
                                        .to_string(),
                                    description: v
                                        .get("description")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    local_dir: env
                                        .plugins_dir()
                                        .join(k)
                                        .to_string_lossy()
                                        .to_string(),
                                    binary_path: v
                                        .get("binary")
                                        .and_then(|b| b.as_str())
                                        .map(|s| s.to_string()),
                                    enabled: v
                                        .get("enabled")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(true),
                                    security_mode: SecurityMode::AlwaysAllow,
                                    execution_mode: ExecutionMode::Auto,
                                    default_turns: -1,
                                    permissions: Vec::new(),
                                    semantic_triggers: Vec::new(),
                                    activation_events: Vec::new(),
                                    capabilities: Vec::new(),
                                    trust_annotations: false,
                                },
                            );
                        }
                    }
                    // Migrate skills
                    if let Some(skills) = legacy_val.get("skills").and_then(|s| s.as_object()) {
                        for (k, v) in skills {
                            reg.installed_tools.insert(
                                k.clone(),
                                ToolEntry {
                                    id: k.clone(),
                                    name: v
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or(k)
                                        .to_string(),
                                    category: "skill".to_string(),
                                    version: v
                                        .get("version")
                                        .and_then(|ver| ver.as_str())
                                        .unwrap_or("1.0.0")
                                        .to_string(),
                                    description: v
                                        .get("description")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    local_dir: env
                                        .skills_dir()
                                        .join(k)
                                        .to_string_lossy()
                                        .to_string(),
                                    binary_path: None,
                                    enabled: v
                                        .get("enabled")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(true),
                                    security_mode: SecurityMode::AlwaysAllow,
                                    execution_mode: ExecutionMode::Auto,
                                    default_turns: -1,
                                    permissions: Vec::new(),
                                    semantic_triggers: Vec::new(),
                                    activation_events: Vec::new(),
                                    capabilities: Vec::new(),
                                    trust_annotations: false,
                                },
                            );
                        }
                    }
                    // Migrate mcp
                    if let Some(mcp) = legacy_val.get("mcp").and_then(|m| m.as_object()) {
                        for (k, v) in mcp {
                            reg.installed_tools.insert(
                                k.clone(),
                                ToolEntry {
                                    id: k.clone(),
                                    name: v
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or(k)
                                        .to_string(),
                                    category: "mcp".to_string(),
                                    version: v
                                        .get("version")
                                        .and_then(|ver| ver.as_str())
                                        .unwrap_or("1.0.0")
                                        .to_string(),
                                    description: v
                                        .get("description")
                                        .and_then(|d| d.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    local_dir: env.mcp_dir().join(k).to_string_lossy().to_string(),
                                    binary_path: None,
                                    enabled: v
                                        .get("enabled")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(true),
                                    security_mode: SecurityMode::RequireApproval,
                                    execution_mode: ExecutionMode::Manual,
                                    default_turns: 3,
                                    permissions: Vec::new(),
                                    semantic_triggers: Vec::new(),
                                    activation_events: Vec::new(),
                                    capabilities: Vec::new(),
                                    trust_annotations: false,
                                },
                            );
                        }
                    }

                    let _ = reg.sync_with_filesystem();
                    let _ = reg.save();
                    tracing::info!(
                        "✅ Successfully migrated legacy registry.yaml into tools_registry.json"
                    );
                    return Ok(reg);
                }
            }
        }

        // 4. Clean Cold boot
        let mut reg = ToolsRegistry::default();
        let _ = reg.sync_with_filesystem();
        let _ = reg.save();
        Ok(reg)
    }

    /// Save the registry to `tools_registry.json` and make it the registry in RAM
    pub fn save(&self) -> Result<()> {
        let json_path = Self::registry_path();
        if let Some(parent) = json_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut updated = self.clone();
        updated.last_updated = chrono::Utc::now().to_rfc3339();

        let json_str = serde_json::to_string_pretty(&updated)?;
        std::fs::write(&json_path, json_str)?;
        store_snapshot(&updated);
        Ok(())
    }

    /// Returns a specific tool entry by ID
    pub fn get_tool(&self, id: &str) -> Option<&ToolEntry> {
        self.installed_tools.get(id)
    }

    /// Returns all registered tools
    pub fn list_tools(&self) -> Vec<ToolEntry> {
        self.installed_tools.values().cloned().collect()
    }

    /// Set a tool's enabled switch and persist changes
    pub fn set_tool_enabled(&mut self, id: &str, enabled: bool) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.enabled = enabled;
            self.save()?;
            Ok(())
        } else {
            anyhow::bail!("Tool '{}' not found in registry", id)
        }
    }

    /// Set a tool's execution mode (Auto/Manual) and persist changes
    pub fn set_tool_execution_mode(&mut self, id: &str, mode: ExecutionMode) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.execution_mode = mode;
            self.save()?;
            Ok(())
        } else {
            anyhow::bail!("Tool '{}' not found in registry", id)
        }
    }

    /// Set a tool's security mode and persist changes
    pub fn set_tool_security_mode(&mut self, id: &str, mode: SecurityMode) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.security_mode = mode;
            self.save()?;
            Ok(())
        } else {
            anyhow::bail!("Tool '{}' not found in registry", id)
        }
    }

    /// Trust (or stop trusting) an MCP server's read-only annotations and persist it
    pub fn set_tool_trust_annotations(&mut self, id: &str, trust: bool) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.trust_annotations = trust;
            self.save()?;
            Ok(())
        } else {
            anyhow::bail!("Tool '{}' not found in registry", id)
        }
    }

    /// Set a tool's default turns and persist changes
    pub fn set_tool_default_turns(&mut self, id: &str, turns: i32) -> Result<()> {
        if let Some(tool) = self.installed_tools.get_mut(id) {
            tool.default_turns = turns;
            self.save()?;
            Ok(())
        } else {
            anyhow::bail!("Tool '{}' not found in registry", id)
        }
    }
}
