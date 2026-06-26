use wasmtime::*;
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::sync::Arc;

use crate::parser::metadata_parser::EngineRules;

lazy_static! {
    /// Global RAM cache for compiled WASM modules. Avoids SSD I/O on every call.
    /// Modules are compiled once and reused across executions.
    static ref WASM_CACHE: DashMap<String, Arc<Module>> = DashMap::new();
}

/// Size of one WASM linear memory page, as defined by the WebAssembly spec.
pub const WASM_PAGE_SIZE: usize = 65536;

/// Per-call resource limiter populated from `EngineRules`.
/// Stored as the Store's data so Wasmtime can enforce limits during execution.
struct PluginStoreLimits {
    /// Maximum linear memory in bytes. `None` = no limit.
    max_memory_bytes: Option<usize>,
}

impl ResourceLimiter for PluginStoreLimits {
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> Result<bool> {
        if let Some(max) = self.max_memory_bytes {
            if desired > max {
                tracing::warn!(
                    "WASM plugin blocked from growing memory to {} bytes — manifest limit is {} bytes.",
                    desired, max
                );
                return Ok(false); // Deny the allocation
            }
        }
        Ok(true)
    }

    fn table_growing(&mut self, _current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool> {
        Ok(true) // Table growth is not restricted
    }
}

/// A sandboxed WASM executor for CEL plugins.
///
/// Execution constraints (fuel limit, memory cap, timeout) are NOT stored here.
/// They are passed per-call from the plugin's `EngineRules`, ensuring that every
/// plugin runs under exactly the limits its manifest declares — no engine-side defaults.
pub struct WasmExecutor {
    engine: Engine,
}

impl Default for WasmExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmExecutor {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.wasm_multi_memory(true);
        // Required for per-execution fuel limits to work at runtime.
        // When fuel_limit is None in manifest, we set u64::MAX (effectively unlimited).
        config.consume_fuel(true);

        Self {
            engine: Engine::new(&config).expect("Failed to initialize WASM Engine"),
        }
    }

    /// Compiles a WASM module from raw bytes and stores it in the global RAM cache.
    /// Subsequent executions reuse the compiled module — zero recompilation overhead.
    pub fn preload_cache(&self, name: &str, wasm_bytes: &[u8]) -> Result<(), String> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| format!("Failed to compile WASM module '{}': {}", name, e))?;
        WASM_CACHE.insert(name.to_string(), Arc::new(module));
        tracing::info!("WASM module '{}' compiled and cached in RAM.", name);
        Ok(())
    }

    /// Executes a loaded WASM plugin with constraints sourced exclusively from `EngineRules`.
    ///
    /// Enforces:
    /// - `fuel_limit` → max instruction count (prevents infinite loops). `None` = u64::MAX.
    /// - `max_memory_mb` → hard RAM cap via `ResourceLimiter` (actually enforced, not just logged).
    ///
    /// No constraint is hardcoded in this function. All limits come from the plugin's manifest.
    pub fn execute_with_rules(
        &self,
        plugin_name: &str,
        payload: &[u8],
        rules: &EngineRules,
    ) -> Result<Vec<u8>, String> {
        let module = WASM_CACHE
            .get(plugin_name)
            .ok_or_else(|| format!("Plugin '{}' not found in RAM cache. Call preload_cache() first.", plugin_name))?;

        // Build the per-call resource limiter from manifest rules (H-2 fix)
        let limits = PluginStoreLimits {
            max_memory_bytes: rules.max_memory_mb.map(|mb| (mb * 1024 * 1024) as usize),
        };

        let mut store = Store::new(&self.engine, limits);

        // Wire the ResourceLimiter — this is what actually enforces memory_growing() denials
        store.limiter(|state| state as &mut dyn ResourceLimiter);

        // Apply fuel limit from manifest — prevents infinite loops.
        // L-3 fix: when fuel_limit is None, set u64::MAX so the engine doesn't fail immediately.
        // (consume_fuel(true) requires fuel to be set before execution.)
        let fuel = rules.fuel_limit.unwrap_or(u64::MAX);
        store.set_fuel(fuel)
            .map_err(|e| format!("Failed to set WASM fuel for '{}': {}", plugin_name, e))?;

        if rules.fuel_limit.is_some() {
            tracing::debug!("WASM '{}': fuel limit set to {} instructions.", plugin_name, fuel);
        }
        if let Some(max_mb) = rules.max_memory_mb {
            tracing::debug!("WASM '{}': memory cap set to {}MB (enforced via ResourceLimiter).", plugin_name, max_mb);
        }
        if rules.allow_network == Some(false) {
            tracing::debug!("WASM '{}': network access denied by manifest.", plugin_name);
        }
        if rules.allow_file_system == Some(false) {
            tracing::debug!("WASM '{}': filesystem access denied by manifest.", plugin_name);
        }

        let linker = Linker::new(&self.engine); // No host functions exposed — absolute sandbox

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("Failed to instantiate WASM module '{}': {}", plugin_name, e))?;

        // 1. Resolve memory allocation hooks exported by the plugin
        let allocate = instance
            .get_typed_func::<u32, i32>(&mut store, "allocate")
            .map_err(|e| format!("'{}' missing 'allocate' export: {}", plugin_name, e))?;
        let deallocate = instance
            .get_typed_func::<(i32, u32), ()>(&mut store, "deallocate")
            .map_err(|e| format!("'{}' missing 'deallocate' export: {}", plugin_name, e))?;
        let execute_cel = instance
            .get_typed_func::<(i32, u32), u64>(&mut store, "execute_cel")
            .map_err(|e| format!("'{}' missing 'execute_cel' export: {}", plugin_name, e))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| format!("'{}' does not export 'memory'.", plugin_name))?;

        // 2. Allocate input buffer inside the WASM sandbox
        let payload_len = payload.len() as u32;
        let ptr = allocate
            .call(&mut store, payload_len)
            .map_err(|e| format!("WASM allocate() failed for '{}': {}", plugin_name, e))?;

        // 3. Write payload into WASM linear memory
        memory
            .write(&mut store, ptr as usize, payload)
            .map_err(|e| format!("WASM memory write failed for '{}': {}", plugin_name, e))?;

        // 4. Execute plugin — fuel consumed here; memory_growing() called during execution
        let ret = execute_cel
            .call(&mut store, (ptr, payload_len))
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("fuel") || msg.contains("Fuel") {
                    format!(
                        "WASM plugin '{}' exceeded its fuel limit ({} instructions). \
                         Increase fuel_limit in the plugin manifest if this is expected.",
                        plugin_name,
                        rules.fuel_limit.unwrap_or(0)
                    )
                } else if msg.contains("memory") || msg.contains("Memory") {
                    format!(
                        "WASM plugin '{}' exceeded its memory limit ({}MB). \
                         Increase max_memory_mb in the plugin manifest if this is expected.",
                        plugin_name,
                        rules.max_memory_mb.unwrap_or(0)
                    )
                } else {
                    format!("WASM execute_cel() failed for '{}': {}", plugin_name, msg)
                }
            })?;

        // 5. Decode packed (ptr, len) return value
        let ret_ptr = (ret >> 32) as i32;
        let ret_len = (ret & 0xFFFFFFFF) as u32;

        // 6. Read output from WASM linear memory
        let mut out_buffer = vec![0u8; ret_len as usize];
        memory
            .read(&mut store, ret_ptr as usize, &mut out_buffer)
            .map_err(|e| format!("WASM output read failed for '{}': {}", plugin_name, e))?;

        // 7. Deallocate both input and output buffers — prevents sandbox memory leaks
        deallocate.call(&mut store, (ptr, payload_len)).ok();
        deallocate.call(&mut store, (ret_ptr, ret_len)).ok();

        Ok(out_buffer)
    }

    /// Serializes an `ExecutionPlan` using Bincode and dispatches it to the WASM plugin.
    /// Rules come from the plugin's manifest — not from this function.
    pub fn execute_plan_with_rules(
        &self,
        plugin_name: &str,
        plan: &crate::parser::planner::ExecutionPlan,
        rules: &EngineRules,
    ) -> Result<Vec<u8>, String> {
        let binary_plan = crate::ffi::cxp_ffi::Transpiler::to_binary_payload(plan)?;
        self.execute_with_rules(plugin_name, &binary_plan, rules)
    }
}
