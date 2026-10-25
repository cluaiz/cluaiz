//! `POST /v1/execute/{component}/{function}`: runs a plugin function. The rest of the old control
//! API (`/v1/control`, `/batch`, `/ops`, `/execute`) is gone: CEP's envelope, batch and
//! `rpc.discover` replace it, and `tools.call` replaces this route in Wave B.

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::AppState;

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
