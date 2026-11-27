//! The MCP client over Streamable HTTP, against a small HTTP server written in this file (no network,
//! no real MCP server). `cargo test -p cluaiz-agents --test mcp_http`

use cluaiz_agents::mcp::McpClient;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// The client keeps at most 4 servers running and stops the least recently used one; tests that run
/// at the same time would stop each other's session, so each test takes this lock first.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The id of the last `tools/call` the `/resume` server saw
static RESUME_CALL_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How many `tools/list` the `/drift` server has answered
static DRIFT_LISTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

struct Reply {
    status: u16,
    content_type: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
}

fn reply(status: u16, content_type: &'static str, body: String) -> Reply {
    Reply { status, content_type, headers: vec![], body }
}

/// What the test server answers, by path:
/// `/json` replies with JSON, `/sse` replies to `tools/call` with an event stream, `/fail` answers
/// `tools/call` with 500, `/html` answers `tools/call` with an HTML page, `/gone` is not found.
fn answer(seen: &Seen) -> Reply {
    let method = seen.body.get("method").and_then(Value::as_str).unwrap_or_default();
    let id = seen.body.get("id").cloned().unwrap_or(Value::Null);
    if seen.path == "/gone" {
        return reply(404, "text/plain", "no such endpoint".into());
    }
    if seen.path == "/resume" && seen.method == "GET" {
        if seen.header("last-event-id") != Some("7") || seen.header("mcp-session-id") != Some("sess-123") {
            return reply(400, "text/plain", "bad resume".into());
        }
        let id = RESUME_CALL_ID.load(std::sync::atomic::Ordering::SeqCst);
        let result = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":"resumed"}]}});
        return reply(200, "text/event-stream", format!("id: 8\ndata: {result}\n\n"));
    }
    if seen.method == "DELETE" {
        return reply(200, "text/plain", String::new());
    }
    // A 2026-07-28 server: no initialize, no session; version and client data in every request
    if seen.path == "/modern" || seen.path == "/params" || seen.path == "/drift" {
        return modern(seen, method, id);
    }
    // A dual-era server that lists only legacy versions at discover
    if seen.path == "/legacy-list" && method == "server/discover" {
        return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32022,"message":"Unsupported protocol version",
            "data":{"supported":["2025-11-25"],"requested":"2026-07-28"}}}).to_string());
    }
    // A server that only knows newer versions
    if seen.path == "/future" {
        return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32022,"message":"Unsupported protocol version",
            "data":{"supported":["2027-01-01"],"requested":"2026-07-28"}}}).to_string());
    }
    if method == "initialize" {
        let mut r = reply(200, "application/json", json!({"jsonrpc":"2.0","id":id,"result":{
            "protocolVersion":"2025-03-26","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1"}}}).to_string());
        r.headers.push(("Mcp-Session-Id", "sess-123"));
        return r;
    }
    // Every later request must carry the session id and the negotiated version
    if seen.header("mcp-session-id") != Some("sess-123") {
        return reply(400, "text/plain", "missing session id".into());
    }
    if seen.header("mcp-protocol-version") != Some("2025-03-26") {
        return reply(400, "text/plain", "bad protocol version".into());
    }
    match method {
        "notifications/initialized" => reply(202, "text/plain", String::new()),
        "tools/list" => reply(200, "application/json", json!({"jsonrpc":"2.0","id":id,"result":{
            "tools":[{"name":"echo","description":"echoes","inputSchema":{"type":"object","properties":{"text":{"type":"string"}}}}]}}).to_string()),
        "tools/call" => {
            let text = seen.body["params"]["arguments"]["text"].as_str().unwrap_or_default().to_string();
            let result = json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":format!("echo:{text}")}]}});
            match seen.path.as_str() {
                // SEP-1699: the stream ends after an event id and a retry hint, with no response; the
                // client must GET with Last-Event-ID to receive it
                "/resume" => {
                    RESUME_CALL_ID.store(id.as_u64().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
                    reply(200, "text/event-stream", "id: 7\nretry: 20\ndata:\n\n".into())
                }
                "/sse" => reply(
                    200,
                    "text/event-stream",
                    format!(
                        "id: 1\ndata: {}\n\nid: 2\r\ndata: {}\r\n\r\n",
                        json!({"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"working"}}),
                        result
                    ),
                ),
                "/fail" => reply(500, "text/plain", "boom".into()),
                "/html" => reply(200, "text/html", "<html>login</html>".into()),
                _ => reply(200, "application/json", result.to_string()),
            }
        }
        _ => reply(400, "text/plain", "unknown method".into()),
    }
}

fn modern(seen: &Seen, method: &str, id: Value) -> Reply {
    let meta = &seen.body["params"]["_meta"];
    if seen.header("mcp-session-id").is_some() || method == "initialize" || method.starts_with("notifications/") {
        return reply(400, "text/plain", "legacy traffic".into());
    }
    if seen.header("mcp-protocol-version") != Some("2026-07-28")
        || meta["io.modelcontextprotocol/protocolVersion"] != "2026-07-28"
        || meta["io.modelcontextprotocol/clientInfo"]["name"] != "cluaiz-engine"
        || !meta["io.modelcontextprotocol/clientCapabilities"].is_object()
        || seen.header("mcp-method") != Some(method)
    {
        return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32020,"message":"Header mismatch"}}).to_string());
    }
    let result = match method {
        "server/discover" => json!({"resultType":"complete","supportedVersions":["2026-07-28"],"capabilities":{"tools":{}},
            "ttlMs":1000,"cacheScope":"private"}),
        "tools/list" if seen.path == "/params" => json!({"resultType":"complete","tools":[
            {"name":"sql","inputSchema":{"type":"object","properties":{
                "region":{"type":"string","x-mcp-header":"Region"},
                "count":{"type":"integer","x-mcp-header":"Count"},
                "flag":{"type":"boolean","x-mcp-header":"Flag"},
                "text":{"type":"string","x-mcp-header":"Text"},
                "skip":{"type":"string","x-mcp-header":"Skip"},
                "opts":{"type":"object","properties":{"level":{"type":"integer","x-mcp-header":"Level"}}},
                "query":{"type":"string"}}}},
            {"name":"dup","inputSchema":{"type":"object","properties":{
                "a":{"type":"string","x-mcp-header":"Same"},"b":{"type":"string","x-mcp-header":"same"}}}},
            {"name":"num","inputSchema":{"type":"object","properties":{"n":{"type":"number","x-mcp-header":"N"}}}}],
            "ttlMs":1000,"cacheScope":"private"}),
        "tools/list" if seen.path == "/drift" => {
            DRIFT_LISTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            json!({"resultType":"complete","tools":[{"name":"echo","inputSchema":{"type":"object"}}],"ttlMs":1000,"cacheScope":"private"})
        }
        "tools/list" => json!({"resultType":"complete","tools":[{"name":"echo","description":"echoes","inputSchema":{"type":"object"}}],
            "ttlMs":1000,"cacheScope":"private"}),
        "tools/call" => {
            if seen.header("mcp-name") != seen.body["params"]["name"].as_str() {
                return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32020,"message":"Mcp-Name mismatch"}}).to_string());
            }
            // "/drift": the schema "changed", so the call fails until the client has listed tools a second time
            // (the first listing is the one made before the first call)
            if seen.path == "/drift" && DRIFT_LISTS.load(std::sync::atomic::Ordering::SeqCst) < 2 {
                return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32020,"message":"Mcp-Param mismatch"}}).to_string());
            }
            if seen.path == "/params" {
                // The server judges the call by its headers: each must match the body, `skip` is null
                let expected = [
                    ("mcp-param-region", "us-west1"),
                    ("mcp-param-count", "42"),
                    ("mcp-param-flag", "true"),
                    ("mcp-param-text", "=?base64?aMOpbGxv?="),
                    ("mcp-param-level", "3"),
                ];
                let wrong = expected.iter().any(|(k, v)| seen.header(k) != Some(*v)) || seen.header("mcp-param-skip").is_some();
                if wrong {
                    return reply(400, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32020,"message":"Mcp-Param mismatch"}}).to_string());
                }
            }
            let text = seen.body["params"]["arguments"]["text"].as_str().unwrap_or_default();
            json!({"resultType":"complete","content":[{"type":"text","text":format!("modern:{text}")}]})
        }
        _ => return reply(404, "application/json", json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}).to_string()),
    };
    reply(200, "application/json", json!({"jsonrpc":"2.0","id":id,"result":result}).to_string())
}

/// Starts the server; returns its address and the list of requests it received
async fn serve() -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let log: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let shared = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            let log = shared.clone();
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head_end, length) = loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&data[..end]).to_ascii_lowercase();
                        let length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while data.len() < head_end + length {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&data[..head_end]).into_owned();
                let mut lines = head.lines();
                let mut first = lines.next().unwrap_or_default().split(' ');
                let (method, path) = (first.next().unwrap_or_default().to_string(), first.next().unwrap_or_default().to_string());
                let headers = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                    .collect();
                let body = serde_json::from_slice(&data[head_end..head_end + length]).unwrap_or(Value::Null);
                let seen = Seen { method, path, headers, body };
                let r = answer(&seen);
                log.lock().unwrap().push(seen);
                let mut out = format!(
                    "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                    r.status, r.content_type, r.body.len()
                );
                for (k, v) in &r.headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("\r\n");
                out.push_str(&r.body);
                let _ = stream.write_all(out.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    (address, log)
}

/// An installed-package folder whose `execution` is the url of the test server
fn package(url: &str, headers: Value) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cluaiz-mcp-http-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), json!({"id":"http-test","execution":{"url":url,"headers":headers}}).to_string()).unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initialize_session_list_and_json_call() {
    let _serial = SERIAL.lock().await;
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/json"), json!({"Authorization": "Bearer abc"}));

    let tools = McpClient::list_tools(&dir).await.unwrap();
    assert_eq!(tools[0]["name"], "echo");
    let result = McpClient::call_tool(&dir, "echo", json!({"text": "hi"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "echo:hi");

    {
        let seen = log.lock().unwrap();
        let init = seen.iter().find(|s| s.body["method"] == "initialize").unwrap();
        assert_eq!(init.method, "POST");
        assert_eq!(init.header("accept"), Some("application/json, text/event-stream"));
        assert_eq!(init.header("authorization"), Some("Bearer abc"));
        assert!(init.header("mcp-session-id").is_none(), "no session id before initialize");
        // The server's session id and the negotiated version ride on every later request
        for later in seen.iter().filter(|s| s.body["method"] != "initialize" && s.body["method"] != "server/discover") {
            assert_eq!(later.header("mcp-session-id"), Some("sess-123"), "{:?}", later.body);
            assert_eq!(later.header("mcp-protocol-version"), Some("2025-03-26"));
            assert_eq!(later.header("authorization"), Some("Bearer abc"));
        }
        let methods: Vec<_> = seen.iter().filter_map(|s| s.body["method"].as_str()).collect();
        // The modern probe goes first; this server is legacy, so it falls back to initialize
        assert_eq!(methods, ["server/discover", "initialize", "notifications/initialized", "tools/list", "tools/call"]);
    }

    // Stopping the server ends the session with a DELETE that carries the session id
    McpClient::stop_server(&dir).await;
    let mut deleted = false;
    for _ in 0..50 {
        deleted = log.lock().unwrap().iter().any(|s| s.method == "DELETE" && s.header("mcp-session-id") == Some("sess-123"));
        if deleted {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(deleted, "no DELETE with the session id was received");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn call_answered_by_an_event_stream() {
    let _serial = SERIAL.lock().await;
    let (address, _log) = serve().await;
    let dir = package(&format!("{address}/sse"), json!({}));
    let result = McpClient::call_tool(&dir, "echo", json!({"text": "stream"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "echo:stream");
    McpClient::stop_server(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_errors_and_non_json_bodies_are_clear_errors() {
    let _serial = SERIAL.lock().await;
    let (address, _log) = serve().await;

    let dir = package(&format!("{address}/fail"), json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("HTTP 500") && error.contains("boom"), "{error}");
    McpClient::stop_server(&dir).await;

    let dir = package(&format!("{address}/html"), json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("text/html") && error.contains("not JSON"), "{error}");
    McpClient::stop_server(&dir).await;

    // The endpoint does not exist: the server never initializes
    let dir = package(&format!("{address}/gone"), json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("did not initialize") && error.contains("HTTP 404"), "{error}");

    // Nothing listens there
    let dir = package("http://127.0.0.1:1/mcp", json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("could not reach"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn header_values_never_leak_into_errors_and_bad_urls_are_refused() {
    let _serial = SERIAL.lock().await;
    let dir = package("ftp://example.test/mcp", json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("http:// or https://"), "{error}");

    let dir = package("http://127.0.0.1:1/mcp?token=SECRET", json!({"Authorization": "Bearer SECRET"}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(!error.contains("SECRET"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stateless_2026_07_28_server_needs_no_initialize_and_no_session() {
    let _serial = SERIAL.lock().await;
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/modern"), json!({"Authorization": "Bearer abc"}));

    let tools = McpClient::list_tools(&dir).await.unwrap();
    assert_eq!(tools[0]["name"], "echo");
    let result = McpClient::call_tool(&dir, "echo", json!({"text": "hi"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "modern:hi");

    McpClient::stop_server(&dir).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let seen = log.lock().unwrap();
    let methods: Vec<_> = seen.iter().filter_map(|s| s.body["method"].as_str()).collect();
    assert_eq!(methods, ["server/discover", "tools/list", "tools/call"], "no initialize, no notifications/initialized");
    for s in seen.iter() {
        assert_eq!(s.header("mcp-protocol-version"), Some("2026-07-28"));
        assert_eq!(s.header("authorization"), Some("Bearer abc"));
        assert!(s.header("mcp-session-id").is_none());
    }
    assert!(seen.iter().all(|s| s.method == "POST"), "a stateless server gets no DELETE");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn modern_probe_falls_back_or_refuses_by_the_servers_modern_error() {
    let _serial = SERIAL.lock().await;
    // A modern error that lists only older versions: the legacy path runs and works
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/legacy-list"), json!({}));
    let result = McpClient::call_tool(&dir, "echo", json!({"text": "old"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "echo:old");
    assert!(log.lock().unwrap().iter().any(|s| s.body["method"] == "initialize"));
    McpClient::stop_server(&dir).await;

    // A server that speaks only newer versions: no fallback, a clear error
    let dir = package(&format!("{address}/future"), json!({}));
    let error = McpClient::call_tool(&dir, "echo", json!({})).await.unwrap_err().to_string();
    assert!(error.contains("2027-01-01"), "{error}");
    assert!(!error.contains("did not initialize"), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn x_mcp_header_values_are_mirrored_and_invalid_tools_are_left_out() {
    let _serial = SERIAL.lock().await;
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/params"), json!({}));

    // A call before any listing: the client reads `tools/list` first to learn the annotations
    let args = json!({"region":"us-west1","count":42,"flag":true,"text":"héllo","skip":null,"opts":{"level":3},"query":"select 1"});
    let result = McpClient::call_tool(&dir, "sql", args).await.unwrap();
    assert_eq!(result["content"][0]["text"], "modern:héllo");
    {
        let seen = log.lock().unwrap();
        let methods: Vec<_> = seen.iter().filter_map(|s| s.body["method"].as_str()).collect();
        assert_eq!(methods, ["server/discover", "tools/list", "tools/call"]);
    }

    // The listing leaves out the two tools whose annotations break the rules, with the others kept
    let tools = McpClient::list_tools(&dir).await.unwrap();
    let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, ["sql"], "duplicate header names and a number parameter are invalid");

    // A value that is missing gets no header (the test server rejects a `Mcp-Param-Skip`)
    let args = json!({"region":"us-west1","count":42,"flag":true,"text":"héllo","opts":{"level":3}});
    McpClient::call_tool(&dir, "sql", args).await.unwrap();
    McpClient::stop_server(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn header_mismatch_relists_tools_and_retries_the_call_once() {
    let _serial = SERIAL.lock().await;
    DRIFT_LISTS.store(0, std::sync::atomic::Ordering::SeqCst);
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/drift"), json!({}));
    let result = McpClient::call_tool(&dir, "echo", json!({"text":"x"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "modern:x");
    let methods: Vec<String> = log.lock().unwrap().iter().filter_map(|s| s.body["method"].as_str().map(str::to_owned)).collect();
    assert_eq!(methods, ["server/discover", "tools/list", "tools/call", "tools/list", "tools/call"]);
    McpClient::stop_server(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn integer_outside_the_safe_range_is_refused_in_an_mcp_param_header() {
    let _serial = SERIAL.lock().await;
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/params"), json!({}));
    let args = json!({"region":"us-west1","count":9007199254740992u64,"flag":true,"text":"x","opts":{"level":3}});
    let error = McpClient::call_tool(&dir, "sql", args).await.unwrap_err().to_string();
    assert!(error.contains("safe range") && error.contains("count"), "{error}");
    assert!(!log.lock().unwrap().iter().any(|s| s.body["method"] == "tools/call"), "the call must not be sent");
    // The largest safe integer is still sent
    let args = json!({"region":"us-west1","count":9007199254740991u64,"flag":true,"text":"x","opts":{"level":3}});
    let error = McpClient::call_tool(&dir, "sql", args).await.map(|_| String::new()).unwrap_or_else(|e| e.to_string());
    assert!(!error.contains("safe range"), "{error}");
    McpClient::stop_server(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closed_event_stream_is_resumed_with_get_and_last_event_id() {
    let _serial = SERIAL.lock().await;
    let (address, log) = serve().await;
    let dir = package(&format!("{address}/resume"), json!({}));
    let result = McpClient::call_tool(&dir, "echo", json!({"text":"x"})).await.unwrap();
    assert_eq!(result["content"][0]["text"], "resumed");
    let seen = log.lock().unwrap();
    let get = seen.iter().find(|s| s.method == "GET").expect("a GET resume");
    assert_eq!(get.header("last-event-id"), Some("7"));
    assert_eq!(get.header("accept"), Some("text/event-stream"));
    drop(seen);
    McpClient::stop_server(&dir).await;
    let _ = std::fs::remove_dir_all(&dir);
}
