//! MCP client over Streamable HTTP. Every JSON-RPC message is one HTTP POST to the server's single MCP
//! endpoint; the reply is one JSON body or an SSE stream. Two eras are spoken: the modern one (spec
//! 2026-07-28: no `initialize`, no session, version and client data in every request's `_meta`) is tried
//! first with `server/discover`; the legacy one (spec 2025-06-18: `initialize`, `Mcp-Session-Id`) is the
//! fallback.

use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";
/// The legacy version asked for at `initialize`; the server may answer with another one (version negotiation)
pub const PROTOCOL_VERSION: &str = "2025-11-25";
/// The newest version supported: stateless, no `initialize` (spec 2026-07-28)
pub const MODERN_VERSION: &str = "2026-07-28";
const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_INFO: &str = "io.modelcontextprotocol/clientInfo";
const META_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// JSON-RPC code of `HeaderMismatch` (spec 2026-07-28)
const HEADER_MISMATCH: i64 = -32020;
/// Most times one reply stream is resumed with `Last-Event-ID`
const MAX_RESUMES: usize = 20;
/// Largest integer JSON can carry exactly: 2^53 - 1
const MAX_SAFE_INTEGER: i128 = 9_007_199_254_740_991;
const PARAM_KEY: &str = "x-mcp-header";
/// Most `tools/list` pages read to learn the `x-mcp-header` annotations before a call
const MAX_LIST_PAGES: usize = 50;
/// Longest piece of an error body put into an error message
const BODY_SNIPPET: usize = 200;

/// A non-success HTTP answer, kept whole so the caller can read the status and body (the spec decides
/// by them whether a server is modern or legacy)
#[derive(Debug)]
struct HttpFailure {
    status: u16,
    body: String,
    message: String,
}

impl std::fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HttpFailure {}

/// A JSON-RPC error object from the server, kept whole for the same reason
#[derive(Debug)]
struct RpcFailure {
    error: Value,
}

impl std::fmt::Display for RpcFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP error: {}", self.error)
    }
}

impl std::error::Error for RpcFailure {}

/// One tool parameter the server asked to be mirrored into an `Mcp-Param-{name}` header
#[derive(Debug, PartialEq)]
struct ParamHeader {
    name: String,
    /// The `properties` keys leading to the parameter inside `arguments`
    path: Vec<String>,
}

/// What the first modern request shows about the server
enum Era {
    Modern,
    Legacy,
}

/// One connection to an HTTP MCP server: the endpoint, the fixed headers from the package, and the
/// `Mcp-Session-Id` the server assigned at `initialize`.
pub struct HttpSession {
    pub name: String,
    /// The endpoint without its query string (query strings often hold tokens), for messages
    shown_url: String,
    url: reqwest::Url,
    headers: HeaderMap,
    client: reqwest::Client,
    workspace: PathBuf,
    session_id: std::sync::Mutex<Option<String>>,
    protocol_version: std::sync::Mutex<Option<String>>,
    /// True when the server speaks 2026-07-28: no session, `_meta` and request headers on every request
    modern: AtomicBool,
    /// The `x-mcp-header` parameters of each tool, learned from `tools/list` (modern servers only)
    tool_params: std::sync::Mutex<HashMap<String, Vec<ParamHeader>>>,
    /// True once this session has read `tools/list`, so `tool_params` is known
    tools_listed: AtomicBool,
    next_id: AtomicU64,
    pub alive: AtomicBool,
    last_used: std::sync::Mutex<Instant>,
    /// Set once a tool call of this session has been answered
    pub warm: AtomicBool,
    /// Called when the server says its tool list changed
    on_list_changed: Arc<dyn Fn() + Send + Sync>,
}

impl HttpSession {
    /// Opens the connection. A modern request (`server/discover`) goes first; a server that is not
    /// modern gets the legacy `initialize` then `notifications/initialized`.
    pub async fn connect(
        name: &str,
        url: &str,
        headers: &HashMap<String, String>,
        workspace: PathBuf,
        on_list_changed: Arc<dyn Fn() + Send + Sync>,
        limit: Duration,
    ) -> Result<Arc<Self>> {
        let parsed = reqwest::Url::parse(url).map_err(|e| anyhow!("MCP server '{name}': the url is not valid: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(anyhow!("MCP server '{name}': the url must start with http:// or https://"));
        }
        let mut shown = parsed.clone();
        shown.set_query(None);
        shown.set_fragment(None);
        let _ = shown.set_username("");
        let _ = shown.set_password(None);
        let mut map = HeaderMap::new();
        for (key, value) in headers {
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|_| anyhow!("MCP server '{name}': '{key}' is not a valid header name"))?;
            // The value is not echoed: it is usually a token
            let mut header_value = HeaderValue::from_str(value)
                .map_err(|_| anyhow!("MCP server '{name}': the value of header '{key}' is not valid"))?;
            header_value.set_sensitive(true);
            map.insert(header_name, header_value);
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            // A redirect would resend custom headers (tokens) to another place and turn a POST into a GET
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| anyhow!("MCP server '{name}': could not create the HTTP client: {e}"))?;
        let session = Arc::new(Self {
            name: name.to_string(),
            shown_url: shown.to_string(),
            url: parsed,
            headers: map,
            client,
            workspace,
            session_id: std::sync::Mutex::new(None),
            protocol_version: std::sync::Mutex::new(Some(MODERN_VERSION.to_string())),
            modern: AtomicBool::new(true),
            tool_params: std::sync::Mutex::new(HashMap::new()),
            tools_listed: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            last_used: std::sync::Mutex::new(Instant::now()),
            warm: AtomicBool::new(false),
            on_list_changed,
        });
        match session.probe_modern(limit).await? {
            Era::Modern => return Ok(session),
            Era::Legacy => {
                session.modern.store(false, Ordering::SeqCst);
                if let Ok(mut version) = session.protocol_version.lock() {
                    *version = None;
                }
            }
        }
        let init = session
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "roots": { "listChanged": true } },
                    "clientInfo": { "name": "cluaiz-engine", "version": cluaiz_core::VERSION }
                }),
                limit,
            )
            .await
            .map_err(|e| anyhow!("MCP server '{}' did not initialize: {}", name, e))?;
        // "The protocol version sent by the client SHOULD be the one negotiated during initialization"
        let negotiated = init.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION).to_string();
        if let Ok(mut version) = session.protocol_version.lock() {
            *version = Some(negotiated);
        }
        session
            .post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), None, false)
            .await
            .map_err(|e| anyhow!("MCP server '{}' did not accept notifications/initialized: {}", name, e))?;
        Ok(session)
    }

    /// The first request: `server/discover` with the modern headers and `_meta`. Spec 2026-07-28
    /// (Streamable HTTP, "Backward Compatibility"): on `400 Bad Request` "inspect the response body
    /// before falling back"; a body with "a recognized modern JSON-RPC error" means a modern server,
    /// anything else means a legacy one, which gets `initialize`.
    async fn probe_modern(self: &Arc<Self>, limit: Duration) -> Result<Era> {
        let outcome = self.request("server/discover", json!({}), limit).await;
        let error = match outcome {
            Ok(result) => {
                let versions = versions_of(result.get("supportedVersions"));
                return if versions.iter().any(|v| v == MODERN_VERSION) {
                    Ok(Era::Modern)
                } else if let Some(newer) = versions.iter().find(|v| v.as_str() > MODERN_VERSION) {
                    Err(anyhow!("MCP server '{}' needs protocol version {} or later, which this client does not speak", self.name, newer))
                } else {
                    // A discover answer without 2026-07-28: not a modern server for this client
                    Ok(Era::Legacy)
                };
            }
            Err(e) => e,
        };
        // The recognized modern error codes: HeaderMismatch -32020, MissingRequiredClientCapability -32021,
        // UnsupportedProtocolVersion -32022
        let rpc = if let Some(rpc) = error.downcast_ref::<RpcFailure>() {
            Some(rpc.error.clone())
        } else {
            error
                .downcast_ref::<HttpFailure>()
                .and_then(|h| serde_json::from_str::<Value>(&h.body).ok())
                .and_then(|body| body.get("error").cloned())
        };
        if let Some(rpc) = &rpc {
            if matches!(rpc.get("code").and_then(Value::as_i64), Some(-32022..=-32020)) {
                let versions = versions_of(rpc.get("data").and_then(|d| d.get("supported")));
                if !versions.is_empty() && !versions.iter().any(|v| v == MODERN_VERSION) {
                    return if versions.iter().any(|v| v.as_str() > MODERN_VERSION) {
                        Err(anyhow!("MCP server '{}' supports only protocol versions {:?}, newer than this client", self.name, versions))
                    } else {
                        // A dual-era server that lists only legacy versions: speak legacy
                        Ok(Era::Legacy)
                    };
                }
                return Err(anyhow!("MCP server '{}' rejected the request: {}", self.name, error));
            }
        }
        // Not a modern error: a 4xx status or a JSON-RPC error (an old server does not know the method)
        let legacy_status = error.downcast_ref::<HttpFailure>().map(|h| (400..500).contains(&h.status)).unwrap_or(false);
        if rpc.is_some() || legacy_status {
            return Ok(Era::Legacy);
        }
        // Not reachable, timeout, 5xx: no era is known
        Err(error)
    }

    pub fn workspace(&self) -> &PathBuf {
        &self.workspace
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.lock().map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Sends a request and waits for its response; a JSON-RPC error becomes `Err`. On a modern server
    /// the `x-mcp-header` annotations of `tools/list` are checked here (an invalid tool is left out of
    /// the result) and remembered, and a `tools/call` first reads them when this session has not yet.
    pub async fn request(self: &Arc<Self>, method: &str, params: Value, limit: Duration) -> Result<Value> {
        let modern = self.modern.load(Ordering::SeqCst);
        if modern && method == "tools/call" && !self.tools_listed.load(Ordering::SeqCst) {
            // The headers of this call depend on the tool's schema; a failed listing is not fatal, the
            // server then judges the call as it is
            if let Err(e) = self.learn_tools(limit).await {
                tracing::warn!("⚠️ [McpClient] '{}' tool list for x-mcp-header could not be read: {}", self.name, e);
            }
        }
        let mut result = match self.send(method, params.clone(), limit).await {
            Err(e) if modern && method == "tools/call" && rpc_code(&e) == Some(HEADER_MISMATCH) => {
                // Spec 2026-07-28 (Streamable HTTP, `HeaderMismatch`): "the client SHOULD call `tools/list` to
                // check for changes to the tool's `inputSchema`, then retry the original request". Once only.
                tracing::warn!("⚠️ [McpClient] '{}' answered HeaderMismatch; reading tools/list and retrying the call once", self.name);
                self.learn_tools(limit).await?;
                self.send(method, params, limit).await?
            }
            other => other?,
        };
        if modern && method == "tools/list" {
            self.screen_tools(&mut result);
        }
        Ok(result)
    }

    /// Reads every `tools/list` page once, only to learn the annotations
    async fn learn_tools(self: &Arc<Self>, limit: Duration) -> Result<()> {
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let params = cursor.as_ref().map(|c| json!({ "cursor": c })).unwrap_or_else(|| json!({}));
            let mut page = self.send("tools/list", params, limit).await?;
            self.screen_tools(&mut page);
            let next = page.get("nextCursor").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_owned);
            if next.is_none() || next == cursor {
                break;
            }
            cursor = next;
        }
        Ok(())
    }

    /// Spec 2026-07-28 (Tools, "x-mcp-header"): "Clients using the Streamable HTTP transport MUST reject
    /// tool definitions where any `x-mcp-header` value violates these constraints. Rejection means the
    /// client MUST exclude the invalid tool from the result of `tools/list`." A valid tool's annotations
    /// are kept for its calls.
    fn screen_tools(&self, result: &mut Value) {
        self.tools_listed.store(true, Ordering::SeqCst);
        let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) else { return };
        let mut known = HashMap::new();
        tools.retain(|tool| {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            match param_annotations(tool.get("inputSchema")) {
                Ok(params) => {
                    known.insert(name, params);
                    true
                }
                Err(reason) => {
                    tracing::warn!("⚠️ [McpClient] '{}' tool '{}' left out: {}", self.name, name, reason);
                    false
                }
            }
        });
        if let Ok(mut map) = self.tool_params.lock() {
            map.extend(known);
        }
    }

    /// The `Mcp-Param-*` headers of one `tools/call` body
    fn param_headers(&self, body: &Value) -> Result<Vec<(String, String)>> {
        let Some(name) = body["params"].get("name").and_then(Value::as_str) else { return Ok(Vec::new()) };
        let map = self.tool_params.lock();
        let Some(params) = map.as_ref().ok().and_then(|m| m.get(name)) else { return Ok(Vec::new()) };
        let mut headers = Vec::new();
        for param in params {
            let Some(mut node) = body["params"].get("arguments") else { break };
            let mut found = true;
            for key in &param.path {
                match node.get(key) {
                    Some(next) => node = next,
                    None => {
                        found = false;
                        break;
                    }
                }
            }
            if !found {
                continue;
            }
            // "Parameter value is null: Client MUST omit the header"
            let text = match node {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => {
                    let Some(i) = n.as_i64().map(i128::from).or_else(|| n.as_u64().map(i128::from)) else { continue };
                    // Spec 2026-07-28: integers in a header "MUST be within the safe range" (-2^53+1 to 2^53-1)
                    if i.abs() > MAX_SAFE_INTEGER {
                        return Err(anyhow!(
                            "MCP tool '{}': integer {} for parameter '{}' is outside the JSON safe range (+-{}); not sent in the Mcp-Param-{} header",
                            name, i, param.path.join("."), MAX_SAFE_INTEGER, param.name
                        ));
                    }
                    i.to_string()
                }
                _ => continue,
            };
            headers.push((format!("mcp-param-{}", param.name), header_value(&text)));
        }
        Ok(headers)
    }

    async fn send(self: &Arc<Self>, method: &str, params: Value, limit: Duration) -> Result<Value> {
        if let Ok(mut last_used) = self.last_used.lock() {
            *last_used = Instant::now();
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let mut params = params;
        if self.modern.load(Ordering::SeqCst) {
            // Spec 2026-07-28: every request carries its version, identity and capabilities in `_meta`
            if !params.is_object() {
                params = json!({});
            }
            params["_meta"] = json!({
                META_VERSION: MODERN_VERSION,
                META_INFO: { "name": "cluaiz-engine", "version": cluaiz_core::VERSION },
                // Nothing offered: no roots, sampling or elicitation, so the server never asks for them
                META_CAPABILITIES: {}
            });
        }
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let exchange = timeout(limit, self.post(&body, Some(id), method == "initialize")).await;
        let response = match exchange {
            Ok(Ok(Some(response))) => response,
            Ok(Ok(None)) => return Err(anyhow!("MCP server '{}' sent no response to {}", self.name, method)),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(anyhow!("Timeout ({}s) waiting for MCP server '{}' to answer {}", limit.as_secs(), self.name, method)),
        };
        if let Some(error) = response.get("error") {
            return Err(anyhow::Error::new(RpcFailure { error: error.clone() }));
        }
        let result = response.get("result").cloned().unwrap_or(Value::Null);
        // "Clients MUST treat results from earlier-protocol servers that omit the field as complete"
        if result.get("resultType").and_then(Value::as_str) == Some("input_required") {
            return Err(anyhow!("MCP server '{}' asked for more input (resultType input_required), which this client does not offer", self.name));
        }
        Ok(result)
    }

    /// One POST. `want_id` is the id of the request in the body: the reply (JSON or SSE) is read until
    /// the response with that id arrives. A notification or a response (`want_id` `None`) expects only
    /// a success status (the spec says 202 with no body).
    async fn post(self: &Arc<Self>, body: &Value, want_id: Option<u64>, is_initialize: bool) -> Result<Option<Value>> {
        let mut request = self
            .client
            .post(self.url.clone())
            .headers(self.headers.clone())
            // The spec: the client "MUST include an Accept header, listing both application/json and text/event-stream"
            .header(ACCEPT, "application/json, text/event-stream")
            .json(body);
        let session_id = self.session_id.lock().ok().and_then(|id| id.clone());
        if let Some(id) = &session_id {
            request = request.header(SESSION_HEADER, id);
        }
        // Legacy: not on `initialize`, the version is only known after it. Modern: always set.
        if let Some(version) = self.protocol_version.lock().ok().and_then(|v| v.clone()) {
            request = request.header(VERSION_HEADER, version);
        }
        if self.modern.load(Ordering::SeqCst) {
            // Spec 2026-07-28, "Standard Request Headers": `Mcp-Method` on all requests, `Mcp-Name` on
            // tools/call, resources/read, prompts/get
            if let Some(method) = body.get("method").and_then(Value::as_str) {
                request = request.header("mcp-method", header_value(method));
                let name_field = match method {
                    "tools/call" | "prompts/get" => Some("name"),
                    "resources/read" => Some("uri"),
                    _ => None,
                };
                if let Some(name) = name_field.and_then(|f| body["params"].get(f)).and_then(Value::as_str) {
                    request = request.header("mcp-name", header_value(name));
                }
                if method == "tools/call" {
                    for (key, value) in self.param_headers(body)? {
                        request = request.header(key, value);
                    }
                }
            }
        }
        let mut response = request
            .send()
            .await
            .map_err(|e| anyhow!("could not reach MCP server '{}' at {}: {}", self.name, self.shown_url, e.without_url()))?;
        let status = response.status();

        if is_initialize {
            if let Some(id) = response.headers().get(SESSION_HEADER).and_then(|v| v.to_str().ok()) {
                if let Ok(mut slot) = self.session_id.lock() {
                    *slot = Some(id.to_string());
                }
            }
        }
        if status == reqwest::StatusCode::NOT_FOUND && session_id.is_some() {
            // The spec: on 404 for a request with a session id, "it MUST start a new session by sending a new InitializeRequest"
            self.alive.store(false, Ordering::SeqCst);
            return Err(anyhow!("MCP server '{}' ended the session (HTTP 404); a new session starts on the next call", self.name));
        }
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            let message = format!("MCP server '{}' answered HTTP {}{}", self.name, status, snippet(&text));
            return Err(anyhow::Error::new(HttpFailure { status: status.as_u16(), body: text, message }));
        }
        let Some(want) = want_id else { return Ok(None) };
        if status == reqwest::StatusCode::ACCEPTED {
            return Err(anyhow!("MCP server '{}' answered HTTP 202 with no response to a request", self.name));
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if content_type.starts_with("text/event-stream") {
            // Spec 2025-11-25 (SEP-1699, Streamable HTTP): a server "MAY close the connection at any time" after
            // an event id; "The client SHOULD then poll the SSE stream by attempting to reconnect", "MUST respect
            // the retry field", resuming "via HTTP GET with Last-Event-ID". Only when an event id was seen.
            let mut last_id: Option<String> = None;
            let mut retry = Duration::ZERO;
            let mut resumes = 0;
            loop {
                let mut buffer = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|e| anyhow!("MCP server '{}' broke the event stream: {}", self.name, e.without_url()))?
                {
                    buffer.extend_from_slice(&chunk);
                    while let Some(event) = take_event(&mut buffer) {
                        note_resume(&event, &mut last_id, &mut retry);
                        if let Some(found) = self.read_event(&event, want) {
                            return Ok(Some(found));
                        }
                    }
                }
                // The stream ended: an event the server did not end with a blank line still counts
                if !buffer.is_empty() {
                    let rest = String::from_utf8_lossy(&buffer).into_owned();
                    note_resume(&rest, &mut last_id, &mut retry);
                    if let Some(found) = self.read_event(&rest, want) {
                        return Ok(Some(found));
                    }
                }
                let Some(id) = last_id.clone().filter(|_| resumes < MAX_RESUMES) else {
                    return Err(anyhow!("MCP server '{}' closed the event stream without a response", self.name));
                };
                resumes += 1;
                tokio::time::sleep(retry).await;
                let mut get = self.client.get(self.url.clone()).headers(self.headers.clone()).header(ACCEPT, "text/event-stream").header("last-event-id", id);
                if let Some(sid) = &session_id {
                    get = get.header(SESSION_HEADER, sid);
                }
                if let Some(version) = self.protocol_version.lock().ok().and_then(|v| v.clone()) {
                    get = get.header(VERSION_HEADER, version);
                }
                response = get
                    .send()
                    .await
                    .map_err(|e| anyhow!("could not resume the event stream of MCP server '{}': {}", self.name, e.without_url()))?;
                let ok_stream = response.status().is_success()
                    && response.headers().get(CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|t| t.to_ascii_lowercase().starts_with("text/event-stream"));
                if !ok_stream {
                    return Err(anyhow!("MCP server '{}' closed the event stream without a response and refused to resume it (HTTP {})", self.name, response.status()));
                }
            }
        }
        let text = response.text().await.map_err(|e| anyhow!("MCP server '{}': could not read the reply: {}", self.name, e.without_url()))?;
        if content_type.starts_with("application/json") || content_type.is_empty() {
            let value: Value = serde_json::from_str(&text)
                .map_err(|_| anyhow!("MCP server '{}' sent a body that is not JSON{}", self.name, snippet(&text)))?;
            return self
                .route_value(value, want)
                .map(Some)
                .ok_or_else(|| anyhow!("MCP server '{}' sent JSON without the response to request {}", self.name, want));
        }
        Err(anyhow!("MCP server '{}' answered with content-type '{}', not JSON or an event stream{}", self.name, content_type, snippet(&text)))
    }

    /// The response to `want` inside one SSE event, if it holds it
    fn read_event(self: &Arc<Self>, event: &str, want: u64) -> Option<Value> {
        let data = event_data(event);
        if data.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<Value>(&data) {
            Ok(value) => self.route_value(value, want),
            Err(_) => {
                tracing::debug!("[MCP {}] event data that is not JSON ignored", self.name);
                None
            }
        }
    }

    /// A JSON value from the server: a message or an array of them (allowed in 2025-03-26). Returns
    /// the response to `want`; server requests are answered and notifications handled on the way.
    fn route_value(self: &Arc<Self>, value: Value, want: u64) -> Option<Value> {
        let messages = match value {
            Value::Array(items) => items,
            other => vec![other],
        };
        let mut found = None;
        for message in messages {
            let method = message.get("method").and_then(Value::as_str).map(str::to_owned);
            let id = message.get("id").cloned();
            match (method, id) {
                (Some(method), Some(id)) => self.answer_server_request(&method, id),
                (Some(method), None) => {
                    if method == "notifications/tools/list_changed" {
                        tracing::info!("🔌 [McpClient] '{}' changed its tool list", self.name);
                        (self.on_list_changed)();
                    } else {
                        tracing::debug!("[MCP {}] notification {}", self.name, method);
                    }
                }
                (None, Some(id)) if id.as_u64() == Some(want) => found = Some(message),
                _ => {}
            }
        }
        found
    }

    /// `roots/list` returns the open workspace; other server requests get "method not found". The
    /// answer is a POST of its own, sent while the stream that carried the request is still open.
    fn answer_server_request(self: &Arc<Self>, method: &str, id: Value) {
        let reply = if method == "roots/list" {
            let name = self.workspace.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            json!({"jsonrpc": "2.0", "id": id, "result": {"roots": [{"uri": super::client::file_uri(&self.workspace), "name": name}]}})
        } else {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("Method not found: {method}")}})
        };
        let session = self.clone();
        tokio::spawn(async move {
            if let Err(e) = session.post(&reply, None, false).await {
                tracing::warn!("⚠️ [McpClient] '{}' could not answer a server request: {}", session.name, e);
            }
        });
    }

    /// Ends the session: the spec says clients "SHOULD send an HTTP DELETE to the MCP endpoint with the
    /// Mcp-Session-Id header" (a server may answer 405). Sent from its own thread so this works from
    /// any context and never blocks the caller; the answer is not awaited.
    pub fn stop(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let Some(id) = self.session_id.lock().ok().and_then(|mut id| id.take()) else { return };
        let (url, headers) = (self.url.clone(), self.headers.clone());
        let version = self.protocol_version.lock().ok().and_then(|v| v.clone());
        std::thread::spawn(move || {
            let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
            else {
                return;
            };
            let mut request = client.delete(url).headers(headers).header(SESSION_HEADER, id);
            if let Some(version) = version {
                request = request.header(VERSION_HEADER, version);
            }
            let _ = request.send();
        });
    }
}

/// The `x-mcp-header` parameters of a tool's `inputSchema`, or the reason the tool is invalid.
/// Rules: spec 2026-07-28, Streamable HTTP, "Custom Headers from Tool Parameters" > "Schema Extension".
fn param_annotations(schema: Option<&Value>) -> std::result::Result<Vec<ParamHeader>, String> {
    let Some(schema) = schema else { return Ok(Vec::new()) };
    let mut found = Vec::new();
    reachable(schema, &mut Vec::new(), &mut found);
    // An annotation anywhere else (inside `items`, `anyOf`, `$defs`, ...) "makes the annotation and thus
    // the tool definition invalid"
    if count_annotations(schema, false) != found.len() {
        return Err("x-mcp-header is not on a property reachable through `properties` keys only".into());
    }
    let mut out: Vec<ParamHeader> = Vec::new();
    for (path, node) in found {
        let name = node.get(PARAM_KEY).and_then(Value::as_str).ok_or("x-mcp-header is not a string")?;
        if name.is_empty() {
            return Err("x-mcp-header is empty".into());
        }
        if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)) {
            return Err(format!("x-mcp-header '{name}' is not an HTTP field-name token"));
        }
        if !matches!(node.get("type").and_then(Value::as_str), Some("string" | "integer" | "boolean")) {
            return Err(format!("x-mcp-header '{name}' is on a parameter that is not a string, integer or boolean"));
        }
        if out.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
            return Err(format!("x-mcp-header '{name}' is used twice (compared without case)"));
        }
        out.push(ParamHeader { name: name.to_string(), path });
    }
    Ok(out)
}

/// Annotated properties reached from `node` through `properties` keys only
fn reachable<'a>(node: &'a Value, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Value)>) {
    let Some(properties) = node.get("properties").and_then(Value::as_object) else { return };
    for (key, child) in properties {
        path.push(key.clone());
        if child.get(PARAM_KEY).is_some() {
            out.push((path.clone(), child));
        }
        reachable(child, path, out);
        path.pop();
    }
}

/// Every `x-mcp-header` keyword in a schema (a property that is merely named so is not one)
fn count_annotations(node: &Value, names_of_properties: bool) -> usize {
    match node {
        Value::Object(map) => {
            let own = usize::from(!names_of_properties && map.contains_key(PARAM_KEY));
            own + map.iter().map(|(key, child)| count_annotations(child, !names_of_properties && key == "properties")).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(|child| count_annotations(child, false)).sum(),
        _ => 0,
    }
}

/// The versions in a JSON array of strings
/// The JSON-RPC error code of a failure, whether it came as a JSON-RPC reply or inside an HTTP 4xx body
fn rpc_code(error: &anyhow::Error) -> Option<i64> {
    let rpc = if let Some(rpc) = error.downcast_ref::<RpcFailure>() {
        rpc.error.clone()
    } else {
        let body = serde_json::from_str::<Value>(&error.downcast_ref::<HttpFailure>()?.body).ok()?;
        body.get("error")?.clone()
    };
    rpc.get("code").and_then(Value::as_i64)
}

fn versions_of(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// An `Mcp-Name` / `Mcp-Method` value: plain when it is visible ASCII and not shaped like the sentinel,
/// else `=?base64?...?=` (spec 2026-07-28, "Value Encoding")
fn header_value(value: &str) -> String {
    let plain = !value.is_empty()
        && value.bytes().all(|b| (0x20..=0x7e).contains(&b))
        && !value.starts_with(' ')
        && !value.ends_with(' ')
        && !(value.starts_with("=?base64?") && value.ends_with("?="));
    if plain {
        value.to_string()
    } else {
        format!("=?base64?{}?=", base64(value.as_bytes()))
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// ": <first part of the body>" for an error message, or nothing when the body is empty
fn snippet(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    let short: String = text.chars().take(BODY_SNIPPET).collect();
    format!(": {}{}", short, if text.chars().count() > BODY_SNIPPET { "..." } else { "" })
}

/// Removes the first complete SSE event (ended by a blank line) from `buffer` and returns its text
fn take_event(buffer: &mut Vec<u8>) -> Option<String> {
    let (end, delimiter) = [b"\r\n\r\n".as_slice(), b"\n\n".as_slice()]
        .iter()
        .filter_map(|delimiter| buffer.windows(delimiter.len()).position(|w| w == *delimiter).map(|at| (at, delimiter.len())))
        .min_by_key(|(at, _)| *at)?;
    let event = String::from_utf8_lossy(&buffer[..end]).into_owned();
    buffer.drain(..end + delimiter);
    Some(event)
}

/// Remembers the `id:` (non-empty) and `retry:` (milliseconds) fields of one SSE event, for a resume
fn note_resume(event: &str, last_id: &mut Option<String>, retry: &mut Duration) {
    for line in event.lines() {
        if let Some(rest) = line.strip_prefix("id:") {
            let id = rest.strip_prefix(' ').unwrap_or(rest).trim();
            if !id.is_empty() {
                *last_id = Some(id.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("retry:") {
            if let Ok(ms) = rest.trim().parse::<u64>() {
                *retry = Duration::from_millis(ms);
            }
        }
    }
}

/// The `data:` lines of one SSE event joined with newlines (the SSE rule); other fields are ignored
fn event_data(event: &str) -> String {
    let mut lines = Vec::new();
    for line in event.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_events_split_on_blank_lines_and_join_data_lines() {
        let mut buffer = b"id: 1\r\ndata: {\"a\":1}\r\n\r\nevent: message\ndata: x\ndata: y\n\ndata: part".to_vec();
        let first = take_event(&mut buffer).unwrap();
        assert_eq!(event_data(&first), "{\"a\":1}");
        let second = take_event(&mut buffer).unwrap();
        assert_eq!(event_data(&second), "x\ny");
        // The last event has no blank line yet: it stays in the buffer
        assert!(take_event(&mut buffer).is_none());
        assert_eq!(buffer, b"data: part");
    }

    #[test]
    fn names_are_plain_or_base64_as_the_spec_shows() {
        assert_eq!(header_value("get_weather"), "get_weather");
        // Examples of the spec's "Encoding examples" table
        assert_eq!(header_value("Hello, 世界"), "=?base64?SGVsbG8sIOS4lueVjA==?=");
        assert_eq!(header_value(" padded "), "=?base64?IHBhZGRlZCA=?=");
        assert_eq!(header_value("line1\nline2"), "=?base64?bGluZTEKbGluZTI=?=");
        assert_eq!(header_value("=?base64?literal?="), "=?base64?PT9iYXNlNjQ/bGl0ZXJhbD89?=");
    }

    fn annotated(schema: Value) -> std::result::Result<Vec<ParamHeader>, String> {
        param_annotations(Some(&schema))
    }

    #[test]
    fn x_mcp_header_valid_schema_gives_paths_including_nested() {
        let params = annotated(json!({"type":"object","properties":{
            "region":{"type":"string","x-mcp-header":"Region"},
            "opts":{"type":"object","properties":{"level":{"type":"integer","x-mcp-header":"Level"}}},
            "query":{"type":"string"}}}))
        .unwrap();
        assert_eq!(params.len(), 2);
        assert!(params.contains(&ParamHeader { name: "Region".into(), path: vec!["region".into()] }));
        assert!(params.contains(&ParamHeader { name: "Level".into(), path: vec!["opts".into(), "level".into()] }));
        assert!(param_annotations(None).unwrap().is_empty());
    }

    #[test]
    fn x_mcp_header_violations_make_the_tool_invalid() {
        let one = |p: Value| json!({"type":"object","properties":{"a":p}});
        assert!(annotated(one(json!({"type":"string","x-mcp-header":""}))).is_err(), "empty");
        assert!(annotated(one(json!({"type":"string","x-mcp-header":"Bad Name"}))).is_err(), "space is not a tchar");
        assert!(annotated(one(json!({"type":"string","x-mcp-header":"A\r\nB"}))).is_err(), "CR LF");
        assert!(annotated(one(json!({"type":"number","x-mcp-header":"N"}))).is_err(), "number not permitted");
        assert!(annotated(one(json!({"type":"object","x-mcp-header":"O"}))).is_err(), "not primitive");
        assert!(annotated(one(json!({"x-mcp-header":"NoType"}))).is_err(), "no type");
        assert!(annotated(one(json!({"type":"string","x-mcp-header":7}))).is_err(), "not a string");
        assert!(annotated(json!({"type":"object","properties":{
            "a":{"type":"string","x-mcp-header":"Dup"},"b":{"type":"string","x-mcp-header":"dup"}}})).is_err(), "case-insensitive duplicate");
        // Not statically reachable: under items, a composition keyword, or $defs
        assert!(annotated(json!({"type":"object","properties":{"list":{"type":"array","items":{"type":"string","x-mcp-header":"I"}}}})).is_err());
        assert!(annotated(json!({"type":"object","anyOf":[{"properties":{"a":{"type":"string","x-mcp-header":"X"}}}]})).is_err());
        assert!(annotated(json!({"type":"object","$defs":{"d":{"type":"string","x-mcp-header":"D"}},"properties":{}})).is_err());
        // A property that is merely named x-mcp-header is not an annotation
        assert!(annotated(json!({"type":"object","properties":{"x-mcp-header":{"type":"string"}}})).unwrap().is_empty());
    }

    #[test]
    fn error_body_snippet_is_short() {
        assert_eq!(snippet("  "), "");
        assert_eq!(snippet("nope"), ": nope");
        assert!(snippet(&"x".repeat(500)).ends_with("..."));
    }
}
