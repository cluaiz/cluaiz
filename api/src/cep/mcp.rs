//! `POST /mcp`: the Engine as an MCP server (Streamable HTTP, JSON responses), built on the CEP
//! registry. A tool is a CEP method with the same name, its params schema
//! as `inputSchema`; a call goes through the same dispatcher as `/v1/cep`. MCP clients act as the
//! `mcp` principal: read-only methods.

use super::CallCtx;
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use cluaiz_cep::envelope::{self, Meta as CallMeta};
use cluaiz_cep::error::METHOD_NOT_FOUND;
use cluaiz_cep::{CepError, OnDenied, Principal, Scope};
use serde_json::{json, Map, Value};
use std::sync::{Arc, LazyLock};

/// Protocol versions this server speaks, newest first
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// MCP clients: read-only methods, anywhere on disk the owner could read (the route already sits
/// behind the owner's authentication setting)
static MCP: LazyLock<Arc<Principal>> = LazyLock::new(|| {
    Arc::new(Principal { id: "mcp".into(), scopes: vec![Scope::Read], roots: None, on_denied: OnDenied::Error, owner: false })
});

fn reply(status: StatusCode, body: Value) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body.to_string()).into_response()
}

fn tools(state: &AppState) -> Vec<Value> {
    state
        .cep
        .registry()
        .methods()
        .filter(|m| m.meta.mcp && MCP.allow(m.meta.scope, m.meta.owner_only).is_ok())
        .map(|m| {
            let mut schema = m.params_schema.clone();
            if let Some(object) = schema.as_object_mut() {
                object.remove("title");
            }
            json!({
                "name": m.name,
                "description": m.meta.summary,
                "inputSchema": schema,
                "annotations": {
                    "readOnlyHint": m.meta.scope == Scope::Read,
                    "destructiveHint": m.meta.destructive,
                    "idempotentHint": m.meta.idempotent,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

/// A method's result or error as an MCP tool result: errors go back to the model (`isError`)
/// so it can correct the call
fn tool_result(outcome: Result<Value, CepError>) -> Value {
    match outcome {
        Ok(value) => {
            let mut result = json!({ "content": [{ "type": "text", "text": value.to_string() }], "isError": false });
            if value.is_object() {
                result["structuredContent"] = value;
            }
            result
        }
        Err(error) => json!({ "content": [{ "type": "text", "text": json!({ "error": error }).to_string() }], "isError": true }),
    }
}

async fn answer(state: &Arc<AppState>, method: &str, params: Map<String, Value>) -> Result<Value, CepError> {
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "cluaiz-engine", "title": "Cluaiz Engine", "version": env!("CARGO_PKG_VERSION") },
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools(state) })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(arguments)) => arguments.clone(),
                Some(_) => return Err(CepError::invalid_params("`arguments` must be an object", Vec::new(), None)),
            };
            let ctx = CallCtx { state: state.clone() };
            match state.cep.invoke(ctx, MCP.clone(), &name, arguments, CallMeta::default()).await {
                Err(error) if error.code == METHOD_NOT_FOUND => Err(CepError::invalid_params(format!("Unknown tool: {name}"), Vec::new(), None)),
                outcome => Ok(tool_result(outcome)),
            }
        }
        other => Err(CepError::method_not_found(other, Vec::new(), Vec::new())),
    }
}

pub async fn handle(State(state): State<Arc<AppState>>, body: Bytes) -> Response {
    // MCP has no batches: one object per POST. Its params may carry MCP's own `_meta`
    // (progressToken…), which is not CEP's, so the message is read here rather than by
    // `envelope::parse`.
    let Ok(Value::Object(mut message)) = serde_json::from_slice::<Value>(&body) else {
        let error = if serde_json::from_slice::<Value>(&body).is_ok() { CepError::invalid_request("MCP takes one JSON object per request") } else { CepError::parse_error() };
        return reply(StatusCode::BAD_REQUEST, envelope::err(Value::Null, &error));
    };
    let id = message.remove("id");
    let method = message.remove("method").and_then(|m| m.as_str().map(str::to_string));
    let (Some(method), true) = (method, message.get("jsonrpc") == Some(&json!("2.0"))) else {
        // A response or a malformed message: nothing for this server to do with it
        return match id {
            Some(id) => reply(StatusCode::OK, envelope::err(id, &CepError::invalid_request("Not a JSON-RPC 2.0 request"))),
            None => StatusCode::ACCEPTED.into_response(),
        };
    };
    let mut params = match message.remove("params") {
        Some(Value::Object(params)) => params,
        _ => Map::new(),
    };
    params.remove("_meta");
    let outcome = answer(&state, &method, params).await;
    match id {
        // A notification (notifications/initialized and the like) is accepted with no body
        None => StatusCode::ACCEPTED.into_response(),
        Some(id) => reply(
            StatusCode::OK,
            match outcome {
                Ok(result) => envelope::ok(id, result),
                Err(error) => envelope::err(id, &error),
            },
        ),
    }
}
