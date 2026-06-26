//! Legacy Interpreter Executor (Rhai)
//!
//! The 4th tier of the execution architecture (after WASM, NATIVE, AUTO_WASM).
//! For rapid prototyping and dynamic scripting without WASM compilation overhead.
//!
//! Limitations vs WASM:
//! - No strict memory isolation (runs in the engine process)
//! - No fuel-based DoS prevention (Rhai has engine limits, but not Wasmtime-level sandboxing)
//! - Intended for trusted internal scripts, not community plugins

use rhai::{Engine, Scope, Dynamic};
use crate::parser::planner::{ExecutionPlan, PlanBlock};

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

    /// Evaluates a Rhai script, injecting the `ExecutionPlan` as readable scope variables.
    ///
    /// Injected variables:
    /// - `plan_block_count: i64` — number of top-level blocks in the plan
    /// - `step_count: i64` — number of steps in the first pipeline block (if any)
    /// - `is_fast_path: bool` — whether the first pipeline is on the fast path
    ///
    /// The script identifier is injected as `plugin_name: String`.
    pub fn execute_script(&self, script: &str, plan: &ExecutionPlan) -> Result<String, String> {
        let mut scope = Scope::new();

        scope.push("plugin_name", "legacy_script".to_string());
        scope.push("plan_block_count", plan.blocks.len() as i64);

        // Inject first pipeline metadata so scripts can inspect the AI's intent
        if let Some(PlanBlock::Pipeline(p)) = plan.blocks.first() {
            scope.push("step_count", p.steps.len() as i64);
            scope.push("is_fast_path", p.is_fast_path);
        } else {
            scope.push("step_count", 0i64);
            scope.push("is_fast_path", false);
        }

        let result: Dynamic = self.engine
            .eval_with_scope(&mut scope, script)
            .map_err(|e| format!("Rhai script execution failed: {}", e))?;

        Ok(result.to_string())
    }
}
