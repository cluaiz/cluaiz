// cluaiz-engine: Core Foundry - Registry
// Manages the lifecycle of cluaiz skills, extensions, plugins, and MCP servers.

pub mod scanner;
pub mod compiler_daemon;
pub mod parser;
pub mod hub_installer;
pub mod download_manager;
pub mod injectors;

// ── Phase A: Two-Tier Registry Architecture ──
// Master registry index (registry.yaml ↔ registry.bin) and lazy-load event bus
pub mod registry_index;
pub mod activation_bus;

// Re-export key types for convenience
pub use registry_index::{MasterRegistry, RegistryEntry, LoadStrategy};
pub use activation_bus::ActivationEventBus;


use std::path::PathBuf;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ComputationalBudget {
    #[serde(default)]
    pub token_length: usize,
    #[serde(default)]
    pub ram_overhead_mb: usize,
    #[serde(default)]
    pub injection_layers: Vec<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct SkillManifest {
    #[serde(default)]
    pub id: String,
    #[serde(alias = "title", default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(alias = "discovery", default)]
    pub triggers: Triggers,
    #[serde(alias = "permissions", default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub computational_budget: Option<ComputationalBudget>,
    #[serde(default)]
    pub user_profile_binding: Option<String>,
    #[serde(default)]
    pub soul_type: String,
    #[serde(default)]
    pub Core_metadata: Option<CoreMetadata>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CoreMetadata {
    pub token_count: usize,
    pub head_dim: usize,
    pub layer_count: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Triggers {
    #[serde(alias = "semantic_triggers", default)]
    pub semantic: Vec<String>,
    pub entropy_threshold: Option<f32>,
    #[serde(default)]
    pub hard_trigger_tokens: Vec<String>,
    #[serde(default)]
    pub cooldown_on_failure_tokens: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Permissions {
    #[serde(default)]
    pub level: String,
    #[serde(alias = "network_access", default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(rename = "type")]
    pub component_type: String,
    pub discovery: ExtensionDiscovery,
    pub activation: ExtensionActivation,
    pub permissions: ExtensionPermissions,
    pub execution: ExtensionExecution,
    pub settings: Option<HashMap<String, SchemaField>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchemaField {
    #[serde(rename = "type")]
    pub field_type: String,
    pub default: Option<serde_json::Value>,
    pub desc: Option<String>,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionDiscovery {
    pub semantic_triggers: Vec<String>,
    pub cel_grammar: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionActivation {
    pub lazy_load: Option<bool>,
    pub trigger_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionPermissions {
    pub max_memory_mb: Option<u32>,
    pub max_cpu_time_ms: Option<u64>,
    pub network_access: Option<bool>,
    pub vram_kv_inject: Option<bool>,
    pub file_system: Option<String>,
    pub mid_layer_jit_injection: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtensionExecution {
    pub envelope: Option<String>,
    pub entry_point: Option<String>,
    pub payload_format: Option<String>,
    pub binary_path: Option<String>,
}

pub struct Skill {
    pub manifest: SkillManifest,
    pub path: PathBuf,
    pub core_tensor_path: Option<PathBuf>, // 🧠 The .tensor Core state path
    pub logic_path: Option<PathBuf>, // ⚙️ The .wasm execution logic path
}

pub struct SkillRegistry {
    pub skills: Vec<Skill>,
    pub router_path: Option<PathBuf>, // 🛰️ The .system/router.bin path
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self { 
            skills: Vec::new(),
            router_path: None,
        }
    }

    pub fn load_from_directory(&mut self, path: &str) {
        let base_path = PathBuf::from(path);
        
        // 🛰️ Initialize System Router
        let router = base_path.join(".system").join("router.bin");
        if router.exists() {
            self.router_path = Some(router);
        }

        let scanner = scanner::SkillScanner::new(path);
        let manifest_paths = scanner.scan_manifests();

        for manifest_path in manifest_paths {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                let parsed_manifest = parser::SkillParser::parse(&manifest_path, &content);

                if let Some(mut manifest) = parsed_manifest {
                    // Fallback ID to name if not provided (common for YAML skills)
                    if manifest.id.is_empty() {
                        manifest.id = manifest.name.clone();
                    }

                    let skill_dir = manifest_path.parent().unwrap();
                    
                    // 🧠 Detect Core Tensor (.tensor)
                    let core_tensor_path = skill_dir.join("core.tensor");
                    let core_tensor = if core_tensor_path.exists() { Some(core_tensor_path) } else { None };

                    // ⚙️ Detect Execution Logic (.wasm)
                    let logic_path = skill_dir.join("logic.wasm");
                    let logic = if logic_path.exists() { Some(logic_path) } else { None };

                    let skill = Skill {
                        manifest: manifest.clone(),
                        path: skill_dir.to_path_buf(),
                        core_tensor_path: core_tensor,
                        logic_path: logic,
                    };
                    
                    // ⚙️ Trigger Sovereign Watcher & Dual-Cache Compilation
                    let daemon = compiler_daemon::CompilerDaemon::new();
                    daemon.compile_skill(&skill.path, &manifest);

                    self.skills.push(skill);
                }
            }
        }
    }
}
