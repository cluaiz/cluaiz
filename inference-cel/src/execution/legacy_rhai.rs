//! Legacy Interpreter Executor (Rhai)
//! This represents the 4th tier of the execution architecture as defined in row.md.
//! It supports executing dynamic Python-like scripts (.rhai) for simple scripting plugins 
//! that don't need the 0.05ms speed of Auto-WASM.

use rhai::{Engine, Scope, Dynamic};
use crate::parser::planner::ExecutionPlan;

pub struct LegacyRhaiExecutor {
    engine: Engine,
}

impl Default for LegacyRhaiExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl LegacyRhaiExecutor {
    pub fn new() -> Self {
        Self {
            engine: Engine::new(),
        }
    }

    /// Evaluates a Rhai script provided as a string, injecting the ExecutionPlan as a variable.
    pub fn execute_script(&self, script: &str, _plan: &ExecutionPlan) -> Result<String, String> {
        let mut scope = Scope::new();
        
        // Note: In a fully implemented version, the `ExecutionPlan` AST would be converted 
        // to a Rhai `Dynamic` map so the script can read the AI's requested actions.
        scope.push("plugin_name", "legacy_script");

        let result: Dynamic = self.engine.eval_with_scope(&mut scope, script)
            .map_err(|e| format!("Rhai Script Execution Failed: {}", e))?;

        Ok(result.to_string())
    }
}
