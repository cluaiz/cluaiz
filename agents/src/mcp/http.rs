//! MCP client over Streamable HTTP (MCP spec 2025-06-18, "Transports"). Every JSON-RPC message is one
//! HTTP POST to the server's single MCP endpoint; the reply is one JSON body or an SSE stream.

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
/// The version asked for at `initialize`; the server may answer with another one (version negotiation)
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Longest piece of an error body put into an error message
const BODY_SNIPPET: usize = 200;

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
    next_id: AtomicU64,
    pub alive: AtomicBool,
    last_used: std::sync::Mutex<Instant>,
    /// Set once a tool call of this session has been answered
    pub warm: AtomicBool,
    /// Called when the server says its tool list changed
    on_list_changed: Arc<dyn Fn() + Send + Sync>,
}

impl HttpSession {
    /// Opens the session: `initialize`, then `notifications/initialized`.
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
            protocol_version: std::sync::Mutex::new(None),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            last_used: std::sync::Mutex::new(Instant::now()),
            warm: AtomicBool::new(false),
            on_list_changed,
        });
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

    pub fn workspace(&self) -> &PathBuf {
        &self.workspace
    }

    pub fn idle_for(&self) -> Duration {
        self.last_used.lock().map(|t| t.elapsed()).unwrap_or_default()
    }

    /// Sends a request and waits for its response; a JSON-RPC error becomes `Err`.
    pub async fn request(self: &Arc<Self>, method: &str, params: Value, limit: Duration) -> Result<Value> {
        if let Ok(mut last_used) = self.last_used.lock() {
            *last_used = Instant::now();
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let exchange = timeout(limit, self.post(&body, Some(id), method == "initialize")).await;
        let response = match exchange {
            Ok(Ok(Some(response))) => response,
            Ok(Ok(None)) => return Err(anyhow!("MCP server '{}' sent no response to {}", self.name, method)),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(anyhow!("Timeout ({}s) waiting for MCP server '{}' to answer {}", limit.as_secs(), self.name, method)),
        };
        if let Some(error) = response.get("error") {
            return Err(anyhow!("MCP error: {}", error));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
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
        // Not on `initialize`: the version is only known after it
        if let Some(version) = self.protocol_version.lock().ok().and_then(|v| v.clone()) {
            request = request.header(VERSION_HEADER, version);
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
            return Err(anyhow!("MCP server '{}' answered HTTP {}{}", self.name, status, snippet(&text)));
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
            let mut buffer = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| anyhow!("MCP server '{}' broke the event stream: {}", self.name, e.without_url()))?
            {
                buffer.extend_from_slice(&chunk);
                while let Some(event) = take_event(&mut buffer) {
                    if let Some(found) = self.read_event(&event, want) {
                        return Ok(Some(found));
                    }
                }
            }
            // The stream ended: an event the server did not end with a blank line still counts
            if !buffer.is_empty() {
                let rest = String::from_utf8_lossy(&buffer).into_owned();
                if let Some(found) = self.read_event(&rest, want) {
                    return Ok(Some(found));
                }
            }
            return Err(anyhow!("MCP server '{}' closed the event stream without a response", self.name));
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
    fn error_body_snippet_is_short() {
        assert_eq!(snippet("  "), "");
        assert_eq!(snippet("nope"), ": nope");
        assert!(snippet(&"x".repeat(500)).ends_with("..."));
    }
}
