use anyhow::Result;
use wasmtime::*;
use tracing::{info, warn};
use crate::neural_foundry::security::permissions::ask_user_permission;

pub struct WasmSandbox {
    engine: Engine,
    linker: Linker<()>,
    store: Store<()>,
}

impl WasmSandbox {
    pub fn new() -> Result<Self> {
        let engine = Engine::default();
        let mut linker = Linker::new(&engine);
        let store = Store::new(&engine, ());
        
        // Register the `cluaiz_host_call` ABI hook so the WASM guest can talk to the host
        linker.func_wrap("env", "cluaiz_host_call", cluaiz_host_call)?;

        Ok(Self { engine, linker, store })
    }

    pub fn initialize_skill(&mut self, skill_wasm_bytes: &[u8]) -> Result<Instance> {
        info!("🛡️ [Sandbox] Compiling and initializing WASM environment for skill...");
        let module = Module::from_binary(&self.engine, skill_wasm_bytes)?;
        let instance = self.linker.instantiate(&mut self.store, &module)?;
        Ok(instance)
    }
}

/// The Holy Bridge: `cluaiz_host_call`
/// This is the ONLY way a sandboxed skill can request OS access.
pub fn cluaiz_host_call(mut caller: Caller<'_, ()>, action_ptr: i32, action_len: i32) -> i32 {
    warn!("🚨 [Host] WASM Skill is requesting system access via cluaiz_host_call");

    // Extract WASM Memory to read the JSON action string
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(mem) => mem,
        None => {
            tracing::error!("Failed to find exported memory");
            return -1;
        }
    };

    let mem_view = memory.data(&caller);
    let start = action_ptr as usize;
    let end = start + action_len as usize;

    if end > mem_view.len() {
        tracing::error!("Memory out of bounds");
        return -1;
    }

    let payload = match std::str::from_utf8(&mem_view[start..end]) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    info!("🛡️ [Sandbox] Intercepted Action Payload: {}", payload);

    // TODO: Parse the JSON payload and execute the specific command securely.
    // E.g. { "action": "bash", "command": "cargo check" }
    
    // Security Interceptor Check
    if !ask_user_permission("cluaiz_host_call_intercept", "Skill wants to run a host command.") {
        warn!("🚫 [Security] Host Action Blocked by User!");
        return -1;
    }

    info!("✅ [Security] Host Action Approved by User. Executing...");
    // Mock Execution Success
    0
}
