use wasmtime::*;
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    /// Global RAM Cache for WASM Plugins. Avoids SSD I/O overhead.
    static ref WASM_CACHE: DashMap<String, Arc<Module>> = DashMap::new();
}

/// A Sandboxed WASM Executor for CEL Plugins.
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
        
        Self {
            engine: Engine::new(&config).expect("Failed to initialize WASM Engine"),
        }
    }

    /// Caches a module into RAM.
    pub fn preload_cache(&self, name: &str, wasm_bytes: &[u8]) -> Result<(), String> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| format!("Failed to compile WASM module: {}", e))?;
        WASM_CACHE.insert(name.to_string(), Arc::new(module));
        Ok(())
    }

    /// Executes the loaded WASM plugin safely using memory allocation hooks with raw bytes.
    pub fn execute(&self, plugin_name: &str, payload: &[u8]) -> Result<Vec<u8>, String> {
        let module = WASM_CACHE.get(plugin_name).ok_or_else(|| format!("Plugin '{}' not found in RAM Cache", plugin_name))?;

        let mut store = Store::new(&self.engine, ());
        let linker = Linker::new(&self.engine); // No host functions exposed, absolute sandbox

        let instance = linker.instantiate(&mut store, &module).map_err(|e| e.to_string())?;

        // 1. Get memory hooks
        let allocate = instance.get_typed_func::<u32, i32>(&mut store, "allocate").map_err(|e| e.to_string())?;
        let deallocate = instance.get_typed_func::<(i32, u32), ()>(&mut store, "deallocate").map_err(|e| e.to_string())?;
        let execute_cel = instance.get_typed_func::<(i32, u32), u64>(&mut store, "execute_cel").map_err(|e| e.to_string())?;
        let memory = instance.get_memory(&mut store, "memory").ok_or("No exported memory")?;

        // 2. Allocate memory inside the WASM sandbox
        let payload_len = payload.len() as u32;
        let ptr = allocate.call(&mut store, payload_len).map_err(|e| e.to_string())?;

        // 3. Inject data safely into WASM linear memory
        memory.write(&mut store, ptr as usize, payload).map_err(|e| e.to_string())?;

        // 4. Execute the plugin native logic
        let ret = execute_cel.call(&mut store, (ptr, payload_len)).map_err(|e| e.to_string())?;
        let ret_ptr = (ret >> 32) as i32;
        let ret_len = (ret & 0xFFFFFFFF) as u32;

        // 5. Read output from WASM memory
        let mut out_buffer = vec![0u8; ret_len as usize];
        memory.read(&mut store, ret_ptr as usize, &mut out_buffer).map_err(|e| e.to_string())?;

        // 6. Deallocate both input and output buffers to prevent memory leaks in sandbox
        deallocate.call(&mut store, (ptr, payload_len)).ok();
        deallocate.call(&mut store, (ret_ptr, ret_len)).ok();

        Ok(out_buffer)
    }

    /// Serializes an ExecutionPlan and sends it to the WASM plugin.
    /// This directly mirrors `cluaizd`'s WASM serialization logic using strict Binary (Bincode) transpilation
    /// rather than slow JSON parsing over the boundary.
    pub fn execute_plan(&self, plugin_name: &str, plan: &crate::parser::planner::ExecutionPlan) -> Result<Vec<u8>, String> {
        // Transpile the ExecutionPlan to strict binary. The plugin DNA will deserialize this natively.
        let binary_plan = crate::ffi::cxp_ffi::Transpiler::to_binary_payload(plan)?;
        
        // Push the compiled bytes directly into the WASM sandbox
        self.execute(plugin_name, &binary_plan)
    }
}
