//! Integration Registry
//! The Universal Router: This module bridges the gap between parsed YAML Metadata 
//! and the actual executable plugin binaries by dynamically routing them based on file extensions.

use dashmap::DashMap;

use crate::parser::metadata_parser::{MetadataParser, Integration};
use crate::execution::{UniversalExecutor, wasm_sandbox::WasmExecutor, native_sandbox::NativeExecutor, legacy_rhai::LegacyRhaiExecutor};
use crate::vram::gpu_injector::inject_from_cpu;

pub struct IntegrationRegistry {
    /// Maps an integration name to its UniversalExecutor instance.
    executors: DashMap<String, UniversalExecutor>,
    /// Stores the parsed metadata and instructions for the AI context.
    integrations: DashMap<String, Integration>,
}

impl Default for IntegrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl IntegrationRegistry {
    pub fn new() -> Self {
        Self {
            executors: DashMap::new(),
            integrations: DashMap::new(),
        }
    }

    /// Loads an Integration by dynamically evaluating the file extensions of its `resolved_links`.
    pub fn load_integration(&self, md_path: &std::path::Path) -> Result<(), String> {
        // 1. Parse the Metadata & Resolve Linked Assets (0ms if .bin cache exists)
        let integration = MetadataParser::parse_file(md_path)?;
        let name = integration.metadata.name.clone();

        let mut executor = None;

        // 2. Dynamically route execution based on resolved file extensions
        for (key, file_path) in &integration.resolved_links {
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

            match ext {
                // Persistent State Memory: Inject directly to VRAM
                "bin" => {
                    tracing::info!("Found Persistent Memory State for '{}' linked as '{}'.", name, key);
                    let state_bytes = std::fs::read(file_path).map_err(|e| e.to_string())?;
                    inject_from_cpu(&state_bytes, "global_kv_cache")?;
                }
                // Native C-FFI Extensions
                "dll" | "so" => {
                    tracing::info!("Found Native FFI Extension for '{}'.", name);
                    #[cfg(any(target_os = "android", target_os = "ios"))]
                    {
                        return Err(format!("Integration '{}' linked a native extension (.{}), which is banned on Mobile OS.", name, ext));
                    }
                    #[cfg(not(any(target_os = "android", target_os = "ios")))]
                    {
                        executor = Some(UniversalExecutor::Native(NativeExecutor::new()));
                    }
                }
                // Sandboxed WASM Logic
                "wasm" => {
                    tracing::info!("Found WASM Logic for '{}'.", name);
                    let wasm_bytes = std::fs::read(file_path).map_err(|e| format!("Failed to read .wasm: {}", e))?;
                    let wasm_exec = WasmExecutor::new();
                    wasm_exec.preload_cache(&name, &wasm_bytes)?;
                    executor = Some(UniversalExecutor::Wasm(wasm_exec));
                }
                // Legacy Scripts
                "rhai" => {
                    tracing::info!("Found Rhai Script for '{}'.", name);
                    executor = Some(UniversalExecutor::Rhai(LegacyRhaiExecutor::new()));
                }
                // JSON/YAML Protocol Connectors (MCP)
                "json" | "yaml" | "yml" => {
                    tracing::info!("Found Connector Protocol (.{}) for '{}'. (Using Legacy executor for now).", ext, name);
                    if executor.is_none() {
                        executor = Some(UniversalExecutor::Rhai(LegacyRhaiExecutor::new()));
                    }
                }
                _ => {
                    tracing::debug!("Ignored unknown link extension '.{}' for '{}'", ext, name);
                }
            }
        }

        // 3. Mount to Universal Engine
        if let Some(exec) = executor {
            self.executors.insert(name.clone(), exec);
            self.integrations.insert(name, integration);
            Ok(())
        } else {
            Err(format!("Integration '{}' had no valid executable links (like .wasm, .dll, or .rhai).", name))
        }
    }

    /// Retrieves the UniversalExecutor for a given integration.
    pub fn get_executor(&self, name: &str) -> Option<dashmap::mapref::one::Ref<'_, String, UniversalExecutor>> {
        self.executors.get(name)
    }

    /// Retrieves the parsed Integration context.
    pub fn get_context(&self, name: &str) -> Option<dashmap::mapref::one::Ref<'_, String, Integration>> {
        self.integrations.get(name)
    }
}
