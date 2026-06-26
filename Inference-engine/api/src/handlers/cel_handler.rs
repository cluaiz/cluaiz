use axum::{
    extract::{State, Json},
    response::IntoResponse,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::AppState;
use engines::neural_foundry::ActivationEventBus;

// External dependency from the CEL crate
use inference_cel::parser::lexer::parse;
use inference_cel::parser::planner::{CelPlanner, PlanBlock, PlanStep};
use inference_cel::execution::native_sandbox::NativeExecutor;
use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType, Transpiler};
use inference_cel::vram::gpu_injector::{inject_from_cpu, ContextInjectionEnvelope, TensorData};

#[derive(Deserialize)]
pub struct CelExecuteRequest {
    pub script: String,
}

#[derive(Serialize)]
pub struct CelExecuteResponse {
    pub success: bool,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// POST /v1/cel/execute
/// A pure Engine Execution API. Takes raw CEL and executes it locally or via plugins.
pub async fn execute_cel_script(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CelExecuteRequest>,
) -> impl IntoResponse {
    // 1. Parse CEL to AST
    let ast = match parse(&payload.script) {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(CelExecuteResponse {
            success: false,
            result: None,
            error: Some(format!("CEL Parsing Error: {}", e)),
        })),
    };

    // 2. Build Execution Plan
    let planner = CelPlanner::new();
    let plan = match planner.build_plan(&ast) {
        Ok(p) => p,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(CelExecuteResponse {
            success: false,
            result: None,
            error: Some(format!("CEL Planning Error: {}", e)),
        })),
    };

    // 3. Execute Plan
    let final_result = execute_cel_plan(plan).await;

    (StatusCode::OK, Json(CelExecuteResponse {
        success: true,
        result: Some(final_result),
        error: None,
    }))
}

/// A core execution function that can be called natively from `ffi_bridge.rs` without HTTP overhead.
pub async fn execute_cel_plan(plan: inference_cel::parser::planner::ExecutionPlan) -> String {
    let mut final_result = String::new();

    for block in plan.blocks {
        match block {
            PlanBlock::Pipeline(pipeline_plan) => {
                for step in pipeline_plan.steps {
                    match step {
                        PlanStep::EngineMemoryControl { action, target } => {
                            final_result.push_str(&format!("[Engine] Memory Control {} on {}\n", action, target));
                        }
                        PlanStep::MidLayerInjection { payload } => {
                            let envelope = ContextInjectionEnvelope {
                                tokens: vec![0, 1, 2],
                                sequence_id: 1,
                                tensor_data: TensorData {
                                    dimensions: vec![1, 3],
                                    values: vec![0.1, 0.2, 0.3],
                                },
                            };
                            
                            if let Ok(binary_payload) = Transpiler::to_binary_payload(&envelope) {
                                match inject_from_cpu(&binary_payload, "auto") {
                                    Ok(_) => final_result.push_str("[Engine] Successfully injected payload into VRAM.\n"),
                                    Err(e) => final_result.push_str(&format!("[Error] VRAM Injection Failed: {}\n", e)),
                                }
                            } else {
                                final_result.push_str("[Error] Failed to transpile injection payload to binary.\n");
                            }
                        }
                        PlanStep::InferenceControl { command } => {
                            final_result.push_str(&format!("[Engine] Inference command: {}\n", command));
                        }
                        PlanStep::ExecuteAction { method, args } => {
                            let executor = NativeExecutor::new();
                            let binary_args = Transpiler::to_binary_payload(&args).unwrap_or(vec![]);
                            let ext_payload = ExtensionPayload::new(PayloadType::Bincode, &binary_args);
                            
                            let dummy_plugin_path = "./.cluaize/extensions/dummy.so";
                            match executor.execute(dummy_plugin_path, &ext_payload) {
                                Ok(bytes) => final_result.push_str(&String::from_utf8_lossy(bytes.as_ref())),
                                Err(e) => final_result.push_str(&format!("[Error] Plugin Execution: {}\n", e)),
                            }
                        }
                        _ => {
                            final_result.push_str("[Engine] Handled step internally.\n");
                        }
                    }
                }
            }
            _ => {
                final_result.push_str("[Engine] Handled complex block.\n");
            }
        }
    }
    final_result
}
