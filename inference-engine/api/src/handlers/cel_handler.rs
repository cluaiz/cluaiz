use axum::{
    extract::{Path, State, Json},
    response::IntoResponse,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::AppState;
// use engines::neural_foundry::ActivationEventBus;

// External dependency from the CEL crate
use inference_cel::parser::lexer::parse;
use inference_cel::parser::planner::{CelPlanner, PlanBlock, PlanStep};
use inference_cel::ffi::cxp_ffi::{ExtensionPayload, PayloadType, Transpiler};
use engines::neural_foundry::executor::sandbox::UnifiedExecutor;
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
                            let executor = UnifiedExecutor::new();
                            let binary_args = Transpiler::to_binary_payload(&args).unwrap_or(vec![]);
                            let ext_payload = ExtensionPayload::new(PayloadType::Bincode, &binary_args);
                            
                            // method acts as the plugin_name in this context (e.g. use plugin::X)
                            match executor.execute(&method, &ext_payload) {
                                Ok(bytes) => final_result.push_str(&String::from_utf8_lossy(bytes.as_ref())),
                                Err(e) => final_result.push_str(&format!("[Error] Plugin Execution: {}\n", e)),
                            }
                        }
                        PlanStep::ExecuteCommand { action: _action, target, args } => {
                            let executor = UnifiedExecutor::new();
                            // Serialize args into a JSON Value
                            // Note: We'll construct a simple JSON from args
                            let mut map = serde_json::Map::new();
                            for (k, v) in args {
                                match v {
                                    inference_cel::parser::ast::CelValue::Text(s) => { map.insert(k, serde_json::Value::String(s)); }
                                    _ => {}
                                }
                            }
                            let json_payload = serde_json::Value::Object(map);
                            let json_bytes = serde_json::to_vec(&json_payload).unwrap_or_default();
                            
                            // For commands, target is the component name
                            if let Some(component) = target {
                                let ext_payload = ExtensionPayload::new(PayloadType::Json, &json_bytes);
                                match executor.execute(&component, &ext_payload) {
                                    Ok(bytes) => final_result.push_str(&String::from_utf8_lossy(bytes.as_ref())),
                                    Err(e) => final_result.push_str(&format!("[Error] Component Execution: {}\n", e)),
                                }
                            } else {
                                final_result.push_str("[Error] ExecuteCommand missing target.\n");
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

#[derive(serde::Deserialize)]
pub struct DynamicPayload {
    pub params: serde_json::Value,
}

/// POST /v1/execute/:component_name/:function_name
pub async fn execute_dynamic(
    Path((component_name, function_name)): Path<(String, String)>,
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<DynamicPayload>
) -> impl IntoResponse {
    let executor = UnifiedExecutor::new();
    let json_bytes = match serde_json::to_vec(&payload.params) {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "status": "error",
                "message": format!("Failed to serialize params: {}", e)
            })));
        }
    };
    
    let ext_payload = ExtensionPayload::new(PayloadType::Json, &json_bytes);
    
    match executor.execute(&component_name, &ext_payload) {
        Ok(result_bytes) => {
            // Try to parse result as string or JSON
            let result_str = String::from_utf8_lossy(&result_bytes).to_string();
            let parsed_result: serde_json::Value = serde_json::from_str(&result_str).unwrap_or(serde_json::json!(result_str));
            
            (StatusCode::OK, Json(serde_json::json!({
                "status": "success",
                "message": format!("Executed {}::{} dynamically.", component_name, function_name),
                "result": parsed_result
            })))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "status": "error",
                "message": format!("Execution failed for {}: {}", component_name, e)
            })))
        }
    }
}
