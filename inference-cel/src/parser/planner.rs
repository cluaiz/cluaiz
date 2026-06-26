use super::ast::{CelOp, CelAst, CompareOp, CelValue, CelPipeline, CelStatement};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step in the execution plan (now part of a nested block execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanStep {
    /// Load a WASM plugin or module into memory
    LoadPlugin {
        name: String,
    },
    
    /// Fast Path Execution for simple text
    FastPathProcess {
        method: String,
        payload_text: String,
    },
    
    /// Invoke a complex action inside the WASM module
    ExecuteAction {
        method: String,
        args: HashMap<String, CelValue>,
    },

    /// Filter logic evaluated natively in Rust before WASM execution
    FilterResults {
        field: String,
        op: CompareOp,
        value: CelValue,
    },

    /// Select fields to reduce RAM overhead
    Select {
        fields: Vec<String>,
    },

    /// Send to VRAM KV Cache
    VectorScan {
        vector: Vec<f32>,
        metric: String,
    },

    /// Manage chat context window
    TimeWindow {
        size: String,
    },
    
    /// Pass result to next plugin
    Pipe {
        next_plugin: String,
    },
    
    /// Generic Command Execution (e.g. CDQL `find User(age: >18)`)
    ExecuteCommand {
        action: String,
        target: Option<String>,
        args: HashMap<String, CelValue>,
    },

    // ── Hardcore Engine Control Steps ──
    EngineMemoryControl {
        action: String,
        target: String,
    },
    MidLayerInjection {
        payload: CelValue,
    },
    InferenceControl {
        command: String,
    },
    SystemCall {
        command: String,
        args: Vec<CelValue>,
    },
}

/// A linear execution plan for a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinePlan {
    pub steps: Vec<PlanStep>,
    pub is_fast_path: bool,
}

impl PipelinePlan {
    pub fn new() -> Self {
        Self { steps: Vec::new(), is_fast_path: false }
    }
}

/// A high-level block node in the Execution Plan (Turing Complete wrapper)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanBlock {
    Pipeline(PipelinePlan),
    Assignment {
        var_name: String,
        pipeline: PipelinePlan,
    },
    IfElse {
        condition: String,
        if_plan: ExecutionPlan,
        else_plan: Option<ExecutionPlan>,
    },
    Foreach {
        item_var: String,
        list_var: String,
        block_plan: ExecutionPlan,
    }
}

/// The full hierarchical execution plan for a CEL script
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub blocks: Vec<PlanBlock>,
}

impl ExecutionPlan {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }
}

pub struct CelPlanner;

impl CelPlanner {
    pub fn new() -> Self {
        Self {}
    }

    /// Converts a full Turing-Complete `CelAst` into an `ExecutionPlan`.
    pub fn build_plan(&self, ast: &CelAst) -> Result<ExecutionPlan, String> {
        let mut plan = ExecutionPlan::new();

        for stmt in &ast.statements {
            let block = match stmt {
                CelStatement::Expression(pipeline) => {
                    PlanBlock::Pipeline(self.build_pipeline(pipeline)?)
                }
                CelStatement::Assignment { var_name, pipeline } => {
                    PlanBlock::Assignment {
                        var_name: var_name.clone(),
                        pipeline: self.build_pipeline(pipeline)?,
                    }
                }
                CelStatement::IfElse { condition, if_block, else_block } => {
                    let if_plan = self.build_plan(if_block)?;
                    let else_plan = match else_block {
                        Some(eb) => Some(self.build_plan(eb)?),
                        None => None,
                    };
                    PlanBlock::IfElse {
                        condition: condition.clone(),
                        if_plan,
                        else_plan,
                    }
                }
                CelStatement::Foreach { item_var, list_var, block } => {
                    PlanBlock::Foreach {
                        item_var: item_var.clone(),
                        list_var: list_var.clone(),
                        block_plan: self.build_plan(block)?,
                    }
                }
            };
            plan.blocks.push(block);
        }

        if plan.blocks.is_empty() {
            return Err("CEL plan has no executable blocks".to_string());
        }

        Ok(plan)
    }

    fn build_pipeline(&self, pipeline: &CelPipeline) -> Result<PipelinePlan, String> {
        let mut plan = PipelinePlan::new();

        for op in &pipeline.ops {
            match op {
                CelOp::ImportPlugin { name } => {
                    plan.steps.push(PlanStep::LoadPlugin { name: name.clone() });
                }
                CelOp::FastProcess { method, payload_text } => {
                    plan.is_fast_path = true;
                    plan.steps.push(PlanStep::FastPathProcess {
                        method: method.clone(),
                        payload_text: payload_text.clone(),
                    });
                }
                CelOp::InvokeAction { method, args } => {
                    plan.steps.push(PlanStep::ExecuteAction {
                        method: method.clone(),
                        args: args.clone(),
                    });
                }
                CelOp::Filter { field, op, value } => {
                    plan.steps.push(PlanStep::FilterResults {
                        field: field.clone(),
                        op: op.clone(),
                        value: value.clone(),
                    });
                }
                CelOp::Select { fields } => {
                    plan.steps.push(PlanStep::Select {
                        fields: fields.clone(),
                    });
                }
                CelOp::SimilarTo { vector, metric } => {
                    plan.steps.push(PlanStep::VectorScan {
                        vector: vector.clone(),
                        metric: metric.clone(),
                    });
                }
                CelOp::TimeWindow { size } => {
                    plan.steps.push(PlanStep::TimeWindow { size: size.clone() });
                }
                CelOp::Pipe { next_plugin } => {
                    plan.steps.push(PlanStep::Pipe { next_plugin: next_plugin.clone() });
                }
                CelOp::Command { action, target, args } => {
                    plan.steps.push(PlanStep::ExecuteCommand {
                        action: action.clone(),
                        target: target.clone(),
                        args: args.clone(),
                    });
                }
                CelOp::EngineMemoryControl { action, target } => {
                    plan.steps.push(PlanStep::EngineMemoryControl {
                        action: action.clone(),
                        target: target.clone(),
                    });
                }
                CelOp::MidLayerInjection { payload } => {
                    plan.steps.push(PlanStep::MidLayerInjection {
                        payload: payload.clone(),
                    });
                }
                CelOp::InferenceControl { command } => {
                    plan.steps.push(PlanStep::InferenceControl {
                        command: command.clone(),
                    });
                }
                CelOp::SystemCall { command, args } => {
                    plan.steps.push(PlanStep::SystemCall {
                        command: command.clone(),
                        args: args.clone(),
                    });
                }
            }
        }

        if plan.steps.is_empty() {
            return Err("CEL pipeline has no executable steps".to_string());
        }

        Ok(plan)
    }
}
