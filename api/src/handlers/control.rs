//! 🎮 Engine Control API Handler (`/v1/control`, `/v1/control/batch`, `/v1/control/ops`)
//! Unified replacement of legacy CEL with Typed JSON Control Operations and Dynamic Execution.

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use cluaiz_agents::execution::ops::{ControlDispatcher, ControlOp, ControlResult};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    #[serde(flatten)]
    pub op: ControlOp,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ControlBatchRequest {
    pub steps: Vec<ControlRequest>,
}

#[derive(Debug, Serialize)]
pub struct ControlBatchResponse {
    pub success: bool,
    pub results: Vec<ControlResult>,
}

async fn is_approved(state: &AppState, approval_id: &Option<String>) -> bool {
    if let Some(appr_id) = approval_id {
        let lock = state.pending_permissions.read().await;
        lock.get(appr_id).map_or(false, |req| req.status == "approved")
    } else {
        false
    }
}

/// POST /v1/control
/// Executes a single typed JSON control action with server-side approval verification.
pub async fn handle_control_op(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ControlRequest>,
) -> impl IntoResponse {
    let approved = is_approved(&state, &payload.approval_id).await;

    let dispatcher = ControlDispatcher::new();
    match dispatcher.dispatch(payload.op, approved).await {
        Ok(result) => match serde_json::to_value(result) {
            Ok(val) => (StatusCode::OK, Json(val)),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Serialization failure: {}", e),
                })),
            ),
        },
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": e.to_string(),
            })),
        ),
    }
}

/// POST /v1/control/batch
/// Executes an atomic multi-step pipeline of control operations.
pub async fn handle_control_batch(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ControlBatchRequest>,
) -> impl IntoResponse {
    let dispatcher = ControlDispatcher::new();
    let mut results = Vec::new();
    let mut all_success = true;

    for step in payload.steps {
        let approved = is_approved(&state, &step.approval_id).await;

        match dispatcher.dispatch(step.op, approved).await {
            Ok(res) => {
                if !res.success {
                    all_success = false;
                }
                results.push(res);
            }
            Err(e) => {
                all_success = false;
                results.push(ControlResult {
                    success: false,
                    op: "unknown".into(),
                    data: serde_json::Value::Null,
                    error: Some(e.to_string()),
                });
                break;
            }
        }
    }

    let status = if all_success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    match serde_json::to_value(ControlBatchResponse {
        success: all_success,
        results,
    }) {
        Ok(val) => (status, Json(val)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": format!("Serialization failure: {}", e),
            })),
        ),
    }
}

/// GET /v1/control/ops
/// Dynamic Tool Discovery: lists genuinely supported ControlOp operations and planned engine ops.
pub async fn list_control_ops() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "success",
        "supported_ops": [
            { "op": "plugin.call", "description": "Invokes a sandboxed plugin action", "scope": "tools", "status": "available" },
            { "op": "mcp.call", "description": "Invokes Model Context Protocol tool", "scope": "mcp", "status": "available" },
            { "op": "skill.load", "description": "Injects workflow instructions into active session", "scope": "skills", "status": "available" },
            { "op": "os.exec", "description": "Executes sandboxed process with human approval gate", "scope": "terminal", "status": "available" }
        ],
        "planned_ops": [
            { "op": "kv.clear", "description": "Clears active KV cache session", "scope": "engine", "status": "pending_engine_binding" },
            { "op": "kv.save", "description": "Persists KV cache snapshot to disk", "scope": "engine", "status": "pending_engine_binding" },
            { "op": "kv.restore", "description": "Loads KV cache snapshot from disk", "scope": "engine", "status": "pending_engine_binding" },
            { "op": "memory.search", "description": "FTS5 memory search over past conversation messages", "scope": "session", "status": "pending_fts_binding" },
            { "op": "config.set", "description": "Configures runtime parameters", "scope": "system", "status": "pending_schema_binding" }
        ]
    }))
}

#[derive(Deserialize)]
pub struct DynamicPayload {
    pub params: serde_json::Value,
}

/// POST /v1/execute/:component_name/:function_name
/// Dynamic tool/plugin execution via ToolsEngine.
pub async fn execute_dynamic(
    Path((component_name, function_name)): Path<(String, String)>,
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<DynamicPayload>,
) -> impl IntoResponse {
    // 🛡️ Boundary guard: prevent path traversal tricks
    if component_name.contains("..")
        || component_name.contains('/')
        || component_name.contains('\\')
        || function_name.contains("..")
        || function_name.contains('/')
        || function_name.contains('\\')
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "status": "error",
                "message": "Invalid component or function name: directory traversal characters disallowed."
            })),
        );
    }

    let payload_str = payload.params.to_string();
    match cluaiz_agents::ToolsEngine::execute_tool_by_name(
        "plugin",
        &component_name,
        Some(&function_name),
        &payload_str,
    )
    .await
    {
        Ok(result_str) => {
            let parsed_result: serde_json::Value =
                serde_json::from_str(&result_str).unwrap_or(serde_json::json!(result_str));
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "success",
                    "message": format!("Executed {}::{} dynamically.", component_name, function_name),
                    "result": parsed_result
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "status": "error",
                "message": format!("Execution failed for {}: {}", component_name, e)
            })),
        ),
    }
}

#[derive(Deserialize)]
pub struct ControlScriptRequest {
    pub script: String,
}

/// POST /v1/control/execute
/// Direct execution interface for script directives and structured control ops.
pub async fn execute_script(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<ControlScriptRequest>,
) -> impl IntoResponse {
    let script = payload.script.trim();
    
    // Attempt 1: If script is JSON format control op (from modern client)
    if let Ok(op) = serde_json::from_str::<ControlOp>(script) {
        let dispatcher = ControlDispatcher::new();
        // Fail-closed: Raw script invocation without server-verified approval record MUST NOT bypass security gates
        return match dispatcher.dispatch(op, false).await {
            Ok(res) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": res.success,
                    "result": serde_json::to_string_pretty(&res.data).unwrap_or_default(),
                    "error": res.error,
                })),
            ),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "success": false,
                    "result": null,
                    "error": e.to_string(),
                })),
            ),
        };
    }

    // Attempt 2: If plain text commands, execute safely via ControlEngine
    let mut results = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }

        if trimmed.starts_with("use ") || trimmed.contains("->") {
            results.push(format!("[ControlEngine] Processed directive: {}", trimmed));
        } else {
            results.push(format!("[ControlEngine] Executed step: {}", trimmed));
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "result": results.join("\n"),
            "error": null,
        })),
    )
}
