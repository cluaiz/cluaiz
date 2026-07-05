use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Defines HOW the engine loads, sandboxes, and limits a component.
/// All values come from the plugin's manifest.yaml — zero engine-side defaults or hardcodes.
///
/// Example manifest.yaml section:
/// ```yaml
/// engine_rules:
///   sandbox_type: "WASM"
///   max_memory_mb: 128
///   fuel_limit: 500000
///   timeout_ms: 30000
///   allow_network: false
///   allow_file_system: false
///   allow_env_vars: false
///   allow_subprocess: false
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EngineRules {
    /// Which runtime sandbox to use: "WASM" | "NATIVE" | "RHAI"
    /// This field — NOT the file extension — is the authority on executor selection.
    pub sandbox_type: String,

    /// Hard RAM cap in megabytes. Engine enforces this via Wasmtime memory limiter.
    /// `None` = no cap (only valid for NATIVE trusted plugins).
    pub max_memory_mb: Option<u64>,

    /// Max WASM instruction count (fuel). Engine passes this to Wasmtime per-execution.
    /// Prevents infinite loops from hanging the engine.
    /// `None` = no fuel limit (only valid for NATIVE/RHAI trusted plugins).
    pub fuel_limit: Option<u64>,

    /// Max wall-clock execution time per CEL call, in milliseconds.
    /// `None` = no timeout enforced.
    pub timeout_ms: Option<u64>,

    /// Whether this plugin is permitted to make outbound network calls.
    /// Wasmtime WASM sandbox blocks network by default; this is an explicit record.
    pub allow_network: Option<bool>,

    /// Whether this plugin is permitted to read/write local filesystem paths.
    pub allow_file_system: Option<bool>,

    /// Whether this plugin is permitted to read environment variables.
    pub allow_env_vars: Option<bool>,

    /// Whether this plugin is permitted to spawn child OS processes.
    pub allow_subprocess: Option<bool>,
}

/// Defines WHERE the engine finds the compiled binary and what ABI version it speaks.
///
/// Example manifest.yaml section:
/// ```yaml
/// ffi_bindings:
///   binary_path: "native/plugin.wasm"
///   entry_point: "execute_cel"
///   abi: "cluaiz-cel-v1"
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FfiBindings {
    /// Relative path to the compiled binary (relative to the manifest's directory).
    pub binary_path: String,

    /// The exported symbol the engine will call (universal CEL boundary function).
    pub entry_point: String,

    /// ABI version string. Engine will reject plugins with incompatible ABI versions.
    pub abi: String,
}

/// Defines how the Engine discovers this extension and how the AI interacts with it.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Discovery {
    /// List of keywords that trigger the engine to lazy-load this extension into the AI's context.
    pub semantic_triggers: Option<Vec<String>>,
    /// The exact CEL syntax the AI must use to invoke this extension.
    pub cel_grammar: Option<String>,
    /// Relative path to the markdown file containing the natural language instructions for the AI.
    pub brain_manual: Option<String>,
}

/// Defines the lifecycle events that cause the Engine to load the binary into memory.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Activation {
    /// If true, the Engine defers loading the binary until an activation event occurs.
    pub lazy_load: Option<bool>,
    /// List of trigger events (e.g., "onCelCommand:use extension::math") that activate the binary.
    pub trigger_on: Option<Vec<String>>,
}

/// Defines how the operating system and Engine should execute the underlying binary.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Execution {
    /// The execution sandbox mode (e.g., "WASM", "NATIVE").
    pub envelope: Option<String>,
    /// The serialized format of the data pointers passed across the FFI boundary (e.g., "MsgPack").
    pub payload_format: Option<String>,
    /// The relative path to the compiled binary file (e.g., "target/release/plugin.wasm").
    pub binary_path: Option<String>,
    /// The name of the exported C-pointer function that the Engine will call (e.g., "execute_cel").
    pub entry_point: Option<String>,
    /// The OS command to execute for out-of-process servers (e.g., "npx", "python")
    pub command: Option<String>,
    /// Arguments to pass to the OS command
    pub args: Option<Vec<String>>,
    /// Environment variables injected into the OS process
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Defines the strict hardware and security sandboxing constraints for execution.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Permissions {
    /// Hard RAM cap in megabytes. The Engine will OOM-kill the sandbox if exceeded.
    pub max_memory_mb: Option<u64>,
    /// Max execution wall-time in milliseconds before the Engine forcefully terminates the call.
    pub max_cpu_time_ms: Option<u64>,
    /// Whether this component is permitted to make outbound HTTP/network calls.
    pub network_access: Option<bool>,
    /// List of explicit domains the component is allowed to contact if network_access is true.
    pub allowed_hosts: Option<Vec<String>>,
    /// DANGEROUS: Whether this plugin is permitted to inject data directly into the LLM's KV Cache.
    pub vram_kv_inject: Option<bool>,
    /// Filesystem access level (e.g., "none", "read_only", "read_write").
    pub file_system: Option<String>,
}

/// The parsed YAML frontmatter of a plugin/skill/extension manifest file.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IntegrationMetadata {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub integration_type: Option<String>,

    pub discovery: Option<Discovery>,
    pub activation: Option<Activation>,
    pub execution: Option<Execution>,
    pub permissions: Option<Permissions>,
    pub settings: Option<serde_json::Value>,
    pub system_bindings: Option<Vec<String>>,

    // Kept for backwards compatibility during migration
    pub links: Option<HashMap<String, String>>,
    pub engine_rules: Option<EngineRules>,
    pub ffi_bindings: Option<FfiBindings>,
}

/// A fully parsed and resolved integration, ready for the execution layer.
#[derive(Serialize, Deserialize, Debug)]
pub struct Integration {
    pub metadata: IntegrationMetadata,
    /// The human-readable instruction body from the manifest (below the `---` frontmatter).
    pub instructions: String,
    /// Absolute paths to all resolved assets defined in the metadata `links`.
    pub resolved_links: HashMap<String, PathBuf>,
}

pub struct MetadataParser;

impl MetadataParser {
    /// Parses any Markdown integration file (e.g., `SKILL.md`, `PLUGIN.md`).
    ///
    /// Expected format:
    /// ```text
    /// ---
    /// name: my-plugin
    /// version: "1.0.0"
    /// engine_rules:
    ///   sandbox_type: "WASM"
    ///   fuel_limit: 500000
    /// ---
    /// Human-readable instructions for the AI here.
    /// ```
    ///
    /// Automatically caches the parsed structure to a `.bin` file for O(1) subsequent reads.
    pub fn parse_file(path: &Path) -> Result<Integration, String> {
        let bin_path = path.with_extension("bin");

        // 1. FAST PATH: Binary Cache (0ms if unchanged)
        if bin_path.exists() {
            let bin_data = fs::read(&bin_path)
                .map_err(|e| format!("Failed to read .bin cache: {}", e))?;
            if let Ok(integration) = bincode::deserialize::<Integration>(&bin_data) {
                tracing::info!("Loaded Integration from binary cache: {:?}", bin_path);
                return Ok(integration);
            }
            // Cache deserialization failed (e.g. schema changed) — fall through to slow path
            tracing::warn!(
                "Binary cache at {:?} is stale or incompatible. Reparsing from source.",
                bin_path
            );
        }

        // 2. SLOW PATH: Parse Markdown + YAML frontmatter
        tracing::info!("Parsing Integration manifest from text: {:?}", path);
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read manifest file: {}", e))?;

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(
                "Invalid manifest format. Expected YAML frontmatter bounded by '---'.".to_string(),
            );
        }

        let yaml_str = parts[1];
        let instructions = parts[2].trim().to_string();

        let metadata: IntegrationMetadata = serde_yaml::from_str(yaml_str)
            .map_err(|e| format!("Failed to parse YAML frontmatter: {}", e))?;

        // 3. Resolve asset links to absolute paths
        let parent_dir = path.parent().unwrap_or(Path::new(""));
        let mut resolved_links = HashMap::new();

        if let Some(links) = &metadata.links {
            for (key, relative_path) in links {
                let absolute_path = parent_dir.join(relative_path);
                if absolute_path.exists() && absolute_path.is_file() {
                    resolved_links.insert(key.clone(), absolute_path);
                } else {
                    tracing::warn!(
                        "Manifest link '{}' → '{}' does not resolve to an existing file.",
                        key,
                        relative_path
                    );
                }
            }
        }

        let integration = Integration {
            metadata,
            instructions,
            resolved_links,
        };

        // 4. Write binary cache for next load
        match bincode::serialize(&integration) {
            Ok(bin_data) => {
                if let Err(e) = fs::write(&bin_path, bin_data) {
                    tracing::warn!("Could not write binary cache to {:?}: {}", bin_path, e);
                }
            }
            Err(e) => {
                tracing::warn!("Could not serialize integration to binary cache: {}", e);
            }
        }

        Ok(integration)
    }
}
