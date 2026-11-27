//! `POST /mcp`: the Engine as an MCP server (Streamable HTTP, JSON responses), built on the CEP
//! registry. A tool is a CEP method with the same name, its params schema
//! as `inputSchema`; a call goes through the same dispatcher as `/v1/cep`. MCP clients act as the
//! `mcp` principal: read-only methods. Layers outside the Engine may add their own tools
//! (`Extension::mcp_tools`): Automation offers the flows the user made tools.

use super::CallCtx;
use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use cluaiz_cep::envelope::{self, Meta as CallMeta};
use cluaiz_cep::error::METHOD_NOT_FOUND;
use cluaiz_cep::{CepError, OnDenied, Principal, Scope};
use serde_json::{json, Map, Value};
use std::sync::{Arc, LazyLock};

/// Protocol versions this server speaks, newest first
const VERSIONS: [&str; 4] = ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];

/// What `initialize` answers when the client asks for a version this server does not speak
/// (unchanged for 2025 clients)
const LEGACY_DEFAULT: &str = "2025-11-25";

/// MCP 2026-07-28: the protocol version rides in every request's `_meta` and in this header
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const VERSION_HEADER: &str = "mcp-protocol-version";

/// `-32022`: the request names a protocol version this server does not speak
const UNSUPPORTED_VERSION: i32 = -32022;

fn capabilities() -> Value {
    json!({ "tools": { "listChanged": false } })
}

fn server_info() -> Value {
    json!({ "name": "cluaiz-engine", "title": "Cluaiz Engine", "version": cluaiz_core::VERSION })
}

/// MCP clients: read-only methods, anywhere on disk the user could read (the route already sits
/// behind the user's authentication setting)
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

/// The Engine's methods and the extensions' tools
async fn all_tools(state: &Arc<AppState>) -> Vec<Value> {
    let mut list = tools(state);
    list.extend(super::extensions::tools(state).await);
    list
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
            let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(LEGACY_DEFAULT);
            Ok(json!({ "protocolVersion": version, "capabilities": capabilities(), "serverInfo": server_info() }))
        }
        "server/discover" => Ok(json!({ "supportedVersions": VERSIONS, "capabilities": capabilities(), "serverInfo": server_info() })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": all_tools(state).await })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(arguments)) => arguments.clone(),
                Some(_) => return Err(CepError::invalid_params("`arguments` must be an object", Vec::new(), None)),
            };
            let caller = super::extensions::Caller { kind: "mcp", chat_id: None, confirmed: false };
            if let Some(call) = super::extensions::call_tool(state, &name, arguments.clone(), caller) {
                return Ok(tool_result(call.await));
            }
            let ctx = CallCtx { state: state.clone() };
            match state.cep.invoke(ctx, MCP.clone(), &name, arguments, CallMeta::default()).await {
                Err(error) if error.code == METHOD_NOT_FOUND => Err(CepError::invalid_params(format!("Unknown tool: {name}"), Vec::new(), None)),
                outcome => Ok(tool_result(outcome)),
            }
        }
        other => Err(CepError::method_not_found(other, Vec::new(), Vec::new())),
    }
}

/// A request that names a protocol version (in `_meta` or the header) must name one we speak.
/// `initialize` negotiates its own version and `server/discover` is how a client finds the
/// list, so neither is refused. No version at all is a 2025 client: accepted as before.
fn check_version(method: &str, meta: Option<&Value>, headers: &HeaderMap) -> Result<(), CepError> {
    if method == "initialize" || method == "server/discover" {
        return Ok(());
    }
    let from_meta = meta.and_then(|m| m.get(META_VERSION)).and_then(Value::as_str);
    let from_header = headers.get(VERSION_HEADER).and_then(|v| v.to_str().ok());
    for asked in [from_meta, from_header].into_iter().flatten() {
        if !VERSIONS.contains(&asked) {
            return Err(CepError::new(UNSUPPORTED_VERSION, format!("Unsupported protocol version: {asked}"), Some(json!({ "supported": VERSIONS, "requested": asked }))));
        }
    }
    Ok(())
}

pub async fn handle(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
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
    let meta = params.remove("_meta");
    // The server keeps no session, so a 2026-07-28 request needs no `initialize` before it
    let outcome = match check_version(&method, meta.as_ref(), &headers) {
        Ok(()) => answer(&state, &method, params).await,
        Err(error) => Err(error),
    };
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
