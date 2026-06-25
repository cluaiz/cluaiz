pub mod wasm_sandbox;
pub mod memory_hooks;
pub mod native_sandbox;
pub mod auto_wasm_compiler;
pub mod legacy_rhai;
pub mod registry;

pub enum UniversalExecutor {
    Wasm(wasm_sandbox::WasmExecutor),
    Native(native_sandbox::NativeExecutor),
    Rhai(legacy_rhai::LegacyRhaiExecutor),
}

impl UniversalExecutor {
    pub fn execute_plan(&self, plugin_identifier: &str, plan: &crate::parser::planner::ExecutionPlan) -> Result<Vec<u8>, String> {
        match self {
            Self::Wasm(executor) => {
                executor.execute_plan(plugin_identifier, plan)
            }
            Self::Native(executor) => {
                // Transpile to strict Binary (Bincode) for zero-allocation C-FFI transfer
                let binary_bytes = crate::ffi::cxp_ffi::Transpiler::to_binary_payload(plan)?;
                let payload = crate::ffi::cxp_ffi::ExtensionPayload::new(
                    crate::ffi::cxp_ffi::PayloadType::Bincode,
                    &binary_bytes
                );
                executor.execute(plugin_identifier, &payload)
            }
            Self::Rhai(executor) => {
                // Load script (assuming plugin_identifier is script source for now)
                let res = executor.execute_script(plugin_identifier, plan)?;
                Ok(res.into_bytes())
            }
        }
    }
}
