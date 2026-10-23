use axum::{Json, extract::State, http::StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use cluaiz_agents::installer::ToolsInstaller;
use super::components::{clear_cache_response, install_component_response, list_cache_response, remove_component_response};

pub async fn list_mcp(State(_state): State<Arc<AppState>>) -> Json<Value> {
    match ToolsInstaller::list_installed_components("mcp") {
        Ok(mcp_servers) => Json(json!({"status": "success", "mcp_servers": mcp_servers})),
        Err(_) => Json(json!({"status": "success", "mcp_servers": []}))
    }
}

#[derive(serde::Deserialize)]
pub struct InstallMcpPayload {
    pub mcp_name: String,
}

pub async fn install_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    install_component_response("mcp", payload.mcp_name).await
}

pub async fn remove_mcp(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    remove_component_response("mcp", payload.mcp_name).await
}

// ─── GET /v1/mcp/cache ─────────────────────────────────────────────
pub async fn list_cache(State(_state): State<Arc<AppState>>) -> (StatusCode, Json<Value>) {
    list_cache_response("mcp")
}

// ─── DELETE /v1/mcp/cache ──────────────────────────────────────────
pub async fn clear_cache(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<InstallMcpPayload>
) -> (StatusCode, Json<Value>) {
    let name = payload.mcp_name;
    let target = (name != "all" && !name.is_empty()).then_some(name);
    clear_cache_response("mcp", target, true)
}

// ─── POST /mcp & /v1/mcp/rpc (Native MCP Server Endpoint) ───────────

#[derive(serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

pub async fn handle_mcp_rpc(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<JsonRpcRequest>,
) -> Json<Value> {
    let id = payload.id.clone().unwrap_or(Value::Null);

    match payload.method.as_str() {
        "initialize" => {
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": { "listChanged": true }
                    },
                    "serverInfo": {
                        "name": "cluaiz-engine",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }
            }))
        }
        "notifications/initialized" | "ping" => {
            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }))
        }
        "tools/list" => {
            let tools = json!([
                {
                    "name": "cluaiz_control",
                    "description": "Executes typed Cluaiz Engine control operations (kv.save, kv.restore, kv.clear, plugin.call, skill.load, os.exec)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "op": { "type": "string", "description": "Control operation name (e.g. kv.save, kv.restore, skill.load, os.exec, plugin.call)" },
                            "args": { "type": "object", "description": "Operation arguments object" }
                        },
                        "required": ["op"]
                    }
                },
                {
                    "name": "cluaiz_hardware_status",
                    "description": "Probes live hardware truth: GPU VRAM, CPU utilization, and RAM buffers",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                },
                {
                    "name": "cluaiz_system_info",
                    "description": "Returns Cluaiz Engine version, architecture, and system runtime info",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                }
            ]);

            Json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": tools
                }
            }))
        }
        "tools/call" => {
            let params = payload.params.unwrap_or(Value::Null);
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            match tool_name {
                "cluaiz_hardware_status" => {
                    let hw = cluaiz_hw::HardwareProber::current();
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&hw).unwrap_or_default()
                                }
                            ]
                        }
                    }))
                }
                "cluaiz_system_info" => {
                    let hw = cluaiz_hw::HardwareProber::current();
                    let info = json!({
                        "engine": "Cluaiz Engine",
                        "version": env!("CARGO_PKG_VERSION"),
                        "cpu": hw.cpu.brand,
                        "ram_available_gb": hw.memory.available_gb,
                        "gpus": hw.gpus
                    });
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [
                                {
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&info).unwrap_or_default()
                                }
                            ]
                        }
                    }))
                }
                "cluaiz_control" => {
                    match serde_json::from_value::<cluaiz_agents::execution::ops::ControlOp>(arguments) {
                        Ok(control_op) => {
                            let dispatcher = cluaiz_agents::execution::ops::ControlDispatcher::new();
                            match dispatcher.dispatch(control_op, true).await {
                                Ok(res) => Json(json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": serde_json::to_string_pretty(&res).unwrap_or_default()
                                            }
                                        ]
                                    }
                                })),
                                Err(e) => Json(json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "result": {
                                        "isError": true,
                                        "content": [
                                            {
                                                "type": "text",
                                                "text": format!("Control dispatch error: {}", e)
                                            }
                                        ]
                                    }
                                }))
                            }
                        }
                        Err(e) => Json(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": format!("Invalid ControlOp arguments: {}", e)
                            }
                        }))
                    }
                }
                _ => Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Unknown tool: {}", tool_name)
                    }
                }))
            }
        }
        _ => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Unsupported method: {}", payload.method)
            }
        }))
    }
}
