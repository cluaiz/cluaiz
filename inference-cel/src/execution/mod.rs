pub mod wasm_sandbox;
pub mod process_sandbox;
pub mod memory_hooks; // kept for now — see note below
pub mod native_sandbox;
pub mod auto_wasm_compiler;
pub mod legacy_rhai;
pub mod registry;
pub mod registry_index;
pub mod activation_bus;
pub mod safety_checker;

use crate::parser::metadata_parser::EngineRules;

/// The unified executor enum for all plugin sandbox types.
pub enum ExtensionExecutor {
    Wasm(wasm_sandbox::WasmExecutor),
    Native(native_sandbox::NativeExecutor),
    Rhai(legacy_rhai::LegacyRhaiExecutor),
}

impl ExtensionExecutor {
    /// Executes an `ExecutionPlan` using constraints from the plugin's `EngineRules`.
    ///
    /// `rules` is always sourced from `integration.metadata.engine_rules` — never hardcoded
    /// in the engine. This ensures each plugin runs under exactly the limits it declares.
    pub fn execute_plan(
        &self,
        plugin_identifier: &str,
        plan: &crate::parser::planner::ExecutionPlan,
        rules: &EngineRules,
    ) -> Result<Vec<u8>, String> {
        match self {
            Self::Wasm(executor) => {
                executor.execute_plan_with_rules(plugin_identifier, plan, rules)
            }

            Self::Native(executor) => {
                // Transpile ExecutionPlan to strict Bincode for zero-allocation C-FFI transfer
                let binary_bytes = crate::ffi::cxp_ffi::Transpiler::to_binary_payload(plan)?;
                let payload = crate::ffi::cxp_ffi::CxpPayload::new(
                    crate::ffi::cxp_ffi::PayloadType::Bincode,
                    &binary_bytes,
                );
                executor.execute_with_rules(plugin_identifier, &payload, rules)
            }

            Self::Rhai(executor) => {
                let res = executor.execute_script(plugin_identifier, plan)?;
                Ok(res.into_bytes())
            }
        }
    }
}
