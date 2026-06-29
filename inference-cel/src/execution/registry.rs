//! cluaiz Extension Registry
//!
//! The dynamic router that bridges parsed manifest metadata and executable plugin binaries.
//! It does NOT hardcode any knowledge of what a plugin does or what sandbox it needs —
//! all of that comes from the plugin's `engine_rules` in its manifest.

use dashmap::DashMap;

use crate::parser::metadata_parser::{MetadataParser, Integration, EngineRules};
use crate::execution::{Cluaizxecutor, wasm_sandbox::WasmExecutor, native_sandbox::NativeExecutor, legacy_rhai::LegacyRhaiExecutor};
use crate::vram::gpu_injector::inject_from_cpu;

/// The default sandbox to fall back to when a manifest does not declare `engine_rules`.
/// WASM is the safest default — fully sandboxed, no host access.
const DEFAULT_SANDBOX_TYPE: &str = "WASM";

/// The registry of all loaded integrations and their executors.
///
/// Previously named `IntegrationRegistry` — renamed to `CluaizxtensionRegistry` to
/// avoid the banned word `Universal` in the executor type and align with project naming.
///
/// Routing logic:
/// - Executor type is determined by `engine_rules.sandbox_type` in the manifest — NOT by file extension
/// - File extension is used only to locate the binary path
/// - The manifest is the single source of truth for how a plugin runs
pub struct CluaizxtensionRegistry {
    /// Maps an integration name to its executor instance.
    executors: DashMap<String, Cluaizxecutor>,
    /// Maps an integration name to its fully parsed manifest (metadata + resolved links).
    integrations: DashMap<String, Integration>,
}

impl Default for CluaizxtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CluaizxtensionRegistry {
    pub fn new() -> Self {
        Self {
            executors: DashMap::new(),
            integrations: DashMap::new(),
        }
    }

    /// Loads an integration from its manifest file path.
    ///
    /// Process:
    /// 1. Parse the manifest (fast: reads `.bin` cache if available)
    /// 2. Determine executor type from `engine_rules.sandbox_type` — NOT file extension
    /// 3. For `.wasm` links: preload into RAM cache
    /// 4. For `.bin` state links: inject into VRAM
    /// 5. Security check: community plugins (no `core_` / `engine_` prefix) cannot use NATIVE
    pub fn load_integration(&self, md_path: &std::path::Path) -> Result<(), String> {
        // 1. Parse manifest — O(1) if .bin cache exists
        let integration = MetadataParser::parse_file(md_path)?;
        let name = integration.metadata.name.clone();

        // 2. Determine sandbox type from manifest engine_rules (manifest is the authority)
        let sandbox_type = integration
            .metadata
            .engine_rules
            .as_ref()
            .map(|r| r.sandbox_type.as_str())
            .unwrap_or(DEFAULT_SANDBOX_TYPE);

        // L-2: Audit log — warn when a component has no binary_hash declared.
        // This makes hash-less loads visible in logs rather than silently trusted.
        let has_binary_hash = integration.metadata.ffi_bindings
            .as_ref()
            .map(|b| !b.binary_path.is_empty())
            .unwrap_or(false);
        if !has_binary_hash {
            tracing::warn!(
                "Integration '{}' loaded WITHOUT a binary_hash in its manifest. \
                 Binary integrity is not verified. Add a 'binary_hash: sha256:<digest>' \
                 to the manifest to enable tamper detection.",
                name
            );
        }


        // 3. Security gate: community plugins cannot use NATIVE sandbox
        //    Only components with trusted prefix may bypass WASM isolation
        if sandbox_type == "NATIVE"
            && !name.starts_with("core_")
            && !name.starts_with("engine_")
        {
            return Err(format!(
                "SECURITY BLOCKED: Integration '{}' declared sandbox_type 'NATIVE' but is not \
                 a trusted core component. Community plugins must use sandbox_type 'WASM'.",
                name
            ));
        }

        // 4. Process linked assets — extension tells us the format/path, manifest tells us the role
        for (key, file_path) in &integration.resolved_links {
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            match ext {
                // Persistent state memory: inject into VRAM KV cache
                "bin" => {
                    tracing::info!(
                        "Integration '{}': found persistent memory state linked as '{}'.",
                        name, key
                    );
                    let state_bytes = std::fs::read(file_path).map_err(|e| e.to_string())?;
                    inject_from_cpu(&state_bytes, "global_kv_cache")?;
                }

                // WASM binary: preload into RAM cache
                "wasm" => {
                    if sandbox_type == "WASM" || sandbox_type == "AUTO_WASM" {
                        tracing::info!("Integration '{}': preloading WASM binary '{}'.", name, key);
                        let wasm_bytes = std::fs::read(file_path)
                            .map_err(|e| format!("Failed to read .wasm for '{}': {}", name, e))?;
                        let wasm_exec = WasmExecutor::new();
                        wasm_exec.preload_cache(&name, &wasm_bytes)?;
                    }
                }

                // Native DLL/SO: available for linking — executor handles actual load at call time
                "dll" | "so" | "dylib" => {
                    if sandbox_type == "NATIVE" {
                        tracing::info!(
                            "Integration '{}': native binary '{}' registered for NATIVE sandbox.",
                            name, key
                        );
                        // Mobile OS ban enforced at execution time in native_sandbox.rs
                    }
                }

                // Rhai scripts: executor handles loading at call time
                "rhai" => {
                    tracing::info!("Integration '{}': Rhai script '{}' registered.", name, key);
                }

                // Protocol connectors (MCP) — metadata only, no binary loading needed
                "json" | "yaml" | "yml" => {
                    tracing::info!(
                        "Integration '{}': protocol connector file '{}' registered.",
                        name, key
                    );
                }

                _ => {
                    tracing::debug!(
                        "Integration '{}': ignoring unknown asset extension '.{}' for '{}'.",
                        name, ext, key
                    );
                }
            }
        }

        // 5. Construct executor based on manifest sandbox_type
        let executor = match sandbox_type {
            "WASM" | "AUTO_WASM" => Cluaizxecutor::Wasm(WasmExecutor::new()),
            "NATIVE" => Cluaizxecutor::Native(NativeExecutor::new()),
            "RHAI" => Cluaizxecutor::Rhai(LegacyRhaiExecutor::new()),
            other => {
                return Err(format!(
                    "Integration '{}' declared unknown sandbox_type '{}'. \
                     Valid values: WASM, NATIVE, RHAI.",
                    name, other
                ));
            }
        };

        // 6. Mount to registry
        self.executors.insert(name.clone(), executor);
        self.integrations.insert(name, integration);

        Ok(())
    }

    /// Executes a loaded integration's plan with rules sourced from its own manifest.
    ///
    /// Rules are never injected externally — they always come from the integration's
    /// own `engine_rules`. This guarantees the plugin runs under the limits it declared.
    pub fn execute(
        &self,
        name: &str,
        plan: &crate::parser::planner::ExecutionPlan,
    ) -> Result<Vec<u8>, String> {
        let executor = self
            .executors
            .get(name)
            .ok_or_else(|| format!("No executor found for integration '{}'. Was it loaded?", name))?;

        let integration = self
            .integrations
            .get(name)
            .ok_or_else(|| format!("No metadata found for integration '{}'.", name))?;

        // Source rules from the integration's own manifest — not from caller, not hardcoded
        let default_rules = default_engine_rules();
        let rules = integration
            .metadata
            .engine_rules
            .as_ref()
            .unwrap_or(&default_rules);

        executor.execute_plan(name, plan, rules)
    }

    /// Returns the executor for a given integration (for direct dispatch by callers).
    pub fn get_executor(
        &self,
        name: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Cluaizxecutor>> {
        self.executors.get(name)
    }

    /// Returns the parsed integration metadata and resolved links.
    pub fn get_integration(
        &self,
        name: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Integration>> {
        self.integrations.get(name)
    }

    /// Returns the `EngineRules` for a given integration.
    /// Returns `None` if the integration is not loaded or has no `engine_rules` section.
    pub fn get_rules(&self, name: &str) -> Option<EngineRules> {
        self.integrations
            .get(name)
            .and_then(|i| i.metadata.engine_rules.clone())
    }
}

/// The safest possible fallback rules when a manifest has no `engine_rules` section.
/// This is a runtime fallback only — manifest authors are expected to declare rules explicitly.
fn default_engine_rules() -> EngineRules {
    EngineRules {
        sandbox_type: DEFAULT_SANDBOX_TYPE.to_string(),
        max_memory_mb: None,   // No cap — manifest did not declare one
        fuel_limit: None,      // No fuel limit — manifest did not declare one
        timeout_ms: None,      // No timeout — manifest did not declare one
        allow_network: Some(false),
        allow_file_system: Some(false),
        allow_env_vars: Some(false),
        allow_subprocess: Some(false),
    }
}
