//! MCP client over stdio. Like other MCP hosts, each server runs as one long-lived process:
//! it is initialized once and then serves any number of requests on the same connection, so
//! several servers and several calls per answer reuse their running processes.

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio::time::timeout;
use super::manifest::McpManifestParser;
use once_cell::sync::Lazy;

/// First start can include a package download (`npx -y`, `uvx`).
const START_TIMEOUT: Duration = Duration::from_secs(90);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// A server with no request for this long is stopped (started again on its next call)
const IDLE_STOP: Duration = Duration::from_secs(10 * 60);
/// Most servers running at once; starting one more stops the least recently used
const MAX_RUNNING: usize = 4;

static MCP_TOOL_CACHE: Lazy<Arc<RwLock<HashMap<PathBuf, Vec<Value>>>>> = Lazy::new(|| {
    Arc::new(RwLock::new(HashMap::new()))
});

/// Running servers, keyed by their package folder.
static SESSIONS: Lazy<Mutex<HashMap<PathBuf, Arc<McpSession>>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Resolves a bare launcher name such as `npx` or `uvx` the way a shell would. On Windows
/// these are `npx.cmd` / `uvx.exe`, which `CreateProcess` does not find from the bare name,
/// so each PATH directory is tried with the PATHEXT extensions. Other inputs pass through.
fn program_path(command: &str) -> PathBuf {
    let path = Path::new(command);
    if !cfg!(windows) || path.extension().is_some() || path.components().count() > 1 {
        return path.to_path_buf();
    }
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .flat_map(|dir| {
            extensions
                .split(';')
                .filter(|ext| !ext.is_empty())
                .map(move |ext| dir.join(format!("{command}{ext}")))
        })
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| path.to_path_buf())
}

/// `file://` URI of a local folder, as MCP roots expect.
fn file_uri(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    for (raw, encoded) in [("%", "%25"), (" ", "%20"), ("#", "%23"), ("?", "%3F")] {
        text = text.replace(raw, encoded);
    }
    if text.starts_with('/') { format!("file://{text}") } else { format!("file:///{text}") }
}

type PendingMap = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// One running MCP server and its JSON-RPC connection.
///
/// The pipes are std's: std reads and writes them with overlapped I/O, which is how it opens
/// its end. Each pipe has one owner — a reader thread for stdout and stderr, and a lock for
/// stdin — so no two operations ever run on the same pipe at once.
struct McpSession {
    name: String,
    /// Package folder (the key of the running servers)
    dir: PathBuf,
    /// When the last request was sent
    last_used: std::sync::Mutex<Instant>,
    /// Workspace the server was started in; a different workspace needs a new process.
    workspace: PathBuf,
    /// `None` once stopped: closing stdin is how a stdio server is told to exit.
    stdin: std::sync::Mutex<Option<ChildStdin>>,
    child: std::sync::Mutex<Child>,
    pending: PendingMap,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
}

impl McpSession {
    async fn start(mcp_dir: &Path) -> Result<Arc<Self>> {
        let manifest = McpManifestParser::parse_file(&mcp_dir.join("package.json"))
            .ok_or_else(|| anyhow!("Failed to parse MCP package.json in {:?}", mcp_dir))?;
        let exec = manifest.execution
            .ok_or_else(|| anyhow!("No execution configuration found for MCP in {:?}", mcp_dir))?;
        if exec.command.trim().is_empty() {
            return Err(anyhow!("MCP execution command is empty for {:?}", mcp_dir));
        }

        let workspace = crate::ToolsEngine::workspace_dir();
        tracing::info!("🔌 [McpClient] Starting MCP server '{}' in {:?}: {} {:?}",
            manifest.name, workspace, exec.command, exec.args);

        let mut cmd = Command::new(program_path(&exec.command));
        // Servers start in the open workspace, as other MCP clients do. The package folder
        // holds Cluaiz's own package.json, which npm would read as an npm project.
        cmd.args(&exec.args)
            .current_dir(&workspace)
            .envs(&exec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("Failed to spawn MCP server '{}' ({}): {}", manifest.name, exec.command, e))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("Failed to open MCP stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("Failed to open MCP stdout"))?;
        if let Some(stderr) = child.stderr.take() {
            let name = manifest.name.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                    tracing::debug!("[MCP {} stderr] {}", name, line);
                }
            });
        }

        let session = Arc::new(Self {
            name: manifest.name.clone(),
            dir: mcp_dir.to_path_buf(),
            last_used: std::sync::Mutex::new(Instant::now()),
            workspace,
            stdin: std::sync::Mutex::new(Some(stdin)),
            child: std::sync::Mutex::new(child),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            alive: Arc::new(AtomicBool::new(true)),
        });

        // One reader owns stdout: responses go to their waiting request, server requests
        // are answered, notifications are logged.
        let reader_session = session.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
                let Ok(message) = serde_json::from_str::<Value>(line.trim()) else { continue };
                let id = message.get("id").cloned();
                match (message.get("method").and_then(|m| m.as_str()), id) {
                    (Some(method), Some(id)) => {
                        if let Err(e) = reader_session.answer_server_request(method, id) {
                            tracing::warn!("⚠️ [McpClient] '{}' could not answer {}: {}", reader_session.name, method, e);
                        }
                    }
                    (Some("notifications/tools/list_changed"), None) => {
                        // The server's functions changed: the next listing asks it again
                        tracing::info!("🔌 [McpClient] '{}' changed its tool list", reader_session.name);
                        MCP_TOOL_CACHE.blocking_write().remove(&reader_session.dir);
                        let _ = std::fs::remove_file(tools_list_cache(&reader_session.dir));
                    }
                    (Some(method), None) => tracing::debug!("[MCP {}] notification {}", reader_session.name, method),
                    (None, Some(id)) => {
                        let waiter = id.as_u64().and_then(|id| reader_session.pending.lock().ok()?.remove(&id));
                        if let Some(waiter) = waiter {
                            let _ = waiter.send(message);
                        }
                    }
                    (None, None) => {}
                }
            }
            // Stdout closed: the server exited. Waiting requests fail when their senders drop.
            reader_session.alive.store(false, Ordering::SeqCst);
            if let Ok(mut pending) = reader_session.pending.lock() {
                pending.clear();
            }
            tracing::info!("🔌 [McpClient] MCP server '{}' exited", reader_session.name);
        });

        session.request("initialize", json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "roots": { "listChanged": true } },
            "clientInfo": { "name": "cluaiz-engine", "version": env!("CARGO_PKG_VERSION") }
        }), START_TIMEOUT).await
            .map_err(|e| anyhow!("MCP server '{}' did not initialize: {}", manifest.name, e))?;
        session.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(session)
    }

    /// Writes one JSON-RPC message line. Lines are small, so the write does not wait on the
    /// server unless its input buffer is full.
    fn send(&self, message: &Value) -> Result<()> {
        let mut line = serde_json::to_string(message)?;
        line.push('\n');
        let mut guard = self.stdin.lock().map_err(|_| anyhow!("MCP stdin lock poisoned"))?;
        let stdin = guard.as_mut().ok_or_else(|| anyhow!("MCP server '{}' was stopped", self.name))?;
        stdin.write_all(line.as_bytes())?;
        stdin.flush()?;
        Ok(())
    }

    /// Sends a request and waits for its response; a JSON-RPC error becomes `Err`.
    async fn request(&self, method: &str, params: Value, limit: Duration) -> Result<Value> {
        if let Ok(mut last_used) = self.last_used.lock() {
            *last_used = Instant::now();
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().map_err(|_| anyhow!("MCP request table poisoned"))?.insert(id, tx);
        if let Err(e) = self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})) {
            self.forget(id);
            return Err(anyhow!("Failed to send {} to MCP server '{}': {}", method, self.name, e));
        }
        let response = match timeout(limit, rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return Err(anyhow!("MCP server '{}' exited before answering {}", self.name, method)),
            Err(_) => {
                self.forget(id);
                return Err(anyhow!("Timeout ({}s) waiting for MCP server '{}' to answer {}", limit.as_secs(), self.name, method));
            }
        };
        if let Some(error) = response.get("error") {
            return Err(anyhow!("MCP error: {}", error));
        }
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    fn forget(&self, id: u64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
    }

    /// `roots/list` returns the open workspace; other server requests get "method not found".
    fn answer_server_request(&self, method: &str, id: Value) -> Result<()> {
        let reply = if method == "roots/list" {
            let name = self.workspace.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            json!({"jsonrpc": "2.0", "id": id, "result": {"roots": [{"uri": file_uri(&self.workspace), "name": name}]}})
        } else {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("Method not found: {method}")}})
        };
        self.send(&reply)
    }

    /// Closes stdin and ends the process. Launchers such as `npx.cmd` run the server as a
    /// child of their own, which exits when its stdin closes.
    fn idle_for(&self) -> Duration {
        self.last_used.lock().map(|t| t.elapsed()).unwrap_or_default()
    }

    fn stop(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut stdin) = self.stdin.lock() {
            stdin.take();
        }
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The running server for `mcp_dir`, started (or restarted after an exit or a workspace
/// change) when needed.
async fn session_for(mcp_dir: &Path) -> Result<Arc<McpSession>> {
    start_idle_reaper();
    let workspace = crate::ToolsEngine::workspace_dir();
    let mut sessions = SESSIONS.lock().await;
    if let Some(existing) = sessions.get(mcp_dir) {
        if existing.alive.load(Ordering::SeqCst) && existing.workspace == workspace {
            return Ok(existing.clone());
        }
        existing.stop();
        sessions.remove(mcp_dir);
    }
    // Room for one more: stop the server that was used least recently
    while sessions.len() >= MAX_RUNNING {
        let Some(oldest) = sessions.iter().max_by_key(|(_, s)| s.idle_for()).map(|(dir, _)| dir.clone()) else {
            break;
        };
        if let Some(stopped) = sessions.remove(&oldest) {
            tracing::info!("🔌 [McpClient] Stopping '{}' to stay within {} running servers", stopped.name, MAX_RUNNING);
            stopped.stop();
        }
    }
    let session = McpSession::start(mcp_dir).await?;
    sessions.insert(mcp_dir.to_path_buf(), session.clone());
    Ok(session)
}

/// Stops servers that have been idle for `IDLE_STOP` (checked every minute on its own thread)
fn start_idle_reaper() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(60));
            let mut sessions = SESSIONS.blocking_lock();
            let idle: Vec<PathBuf> = sessions
                .iter()
                .filter(|(_, session)| session.idle_for() >= IDLE_STOP || !session.alive.load(Ordering::SeqCst))
                .map(|(dir, _)| dir.clone())
                .collect();
            for dir in idle {
                if let Some(session) = sessions.remove(&dir) {
                    tracing::info!("🔌 [McpClient] Stopping idle MCP server '{}'", session.name);
                    session.stop();
                }
            }
        });
    });
}

/// Subprocess IPC client communicating with external MCP servers over stdio with full JSON-RPC 2.0 protocol handshake
pub struct McpClient;

impl McpClient {
    /// Discovers tools from an MCP server conforming to JSON-RPC 2.0 `tools/list`
    pub async fn list_tools(mcp_dir: &Path) -> Result<Vec<Value>> {
        let manifest_path = mcp_dir.join("package.json");

        // 1. Check in-memory cache first
        let cache_key = mcp_dir.to_path_buf();
        {
            let cache = MCP_TOOL_CACHE.read().await;
            if let Some(cached_tools) = cache.get(&cache_key) {
                return Ok(cached_tools.clone());
            }
        }

        // 2. Check if static tools exist in package.json
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(tools_arr) = val.get("tools").and_then(|t| t.as_array()) {
                    if !tools_arr.is_empty() {
                        let tools = tools_arr.clone();
                        let mut cache = MCP_TOOL_CACHE.write().await;
                        cache.insert(cache_key, tools.clone());
                        return Ok(tools);
                    }
                }
            }
        }

        // 3. Ask the running server
        let session = session_for(mcp_dir).await?;
        let result = session.request("tools/list", json!({}), REQUEST_TIMEOUT).await?;
        let tools = result.get("tools").and_then(|t| t.as_array()).cloned().unwrap_or_default();

        // Kept on disk so tool matching knows the functions after a restart, without starting
        // the server. It is only read for matching; loading always asks the running server.
        let saved = tools_list_cache(mcp_dir);
        if let Some(dir) = saved.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&saved, serde_json::to_vec(&tools).unwrap_or_default()) {
            tracing::warn!("⚠️ [McpClient] Could not save the tool list of {:?}: {}", mcp_dir, e);
        }

        let mut cache = MCP_TOOL_CACHE.write().await;
        cache.insert(cache_key, tools.clone());
        Ok(tools)
    }

    /// Clears the discovered tool cache
    pub async fn clear_tool_cache() {
        let mut cache = MCP_TOOL_CACHE.write().await;
        cache.clear();
    }

    /// Stops the running server of `mcp_dir` (if any) and forgets its tools, e.g. when the
    /// package is removed or replaced.
    pub async fn stop_server(mcp_dir: &Path) {
        if let Some(session) = SESSIONS.lock().await.remove(mcp_dir) {
            session.stop();
        }
        MCP_TOOL_CACHE.write().await.remove(mcp_dir);
    }

    /// Calls a tool on the server's running process
    pub async fn call_tool(mcp_dir: &Path, tool_name: &str, arguments: Value) -> Result<Value> {
        let session = session_for(mcp_dir).await?;
        let result = session.request("tools/call", json!({ "name": tool_name, "arguments": arguments }), REQUEST_TIMEOUT).await
            .map_err(|e| anyhow!("MCP tool error: {}", e))?;

        // A tool-level failure is a result with `isError: true` (MCP spec).
        if result.get("isError").and_then(|e| e.as_bool()) == Some(true) {
            let texts: Vec<&str> = result.get("content").and_then(|c| c.as_array())
                .map(|items| items.iter().filter_map(|c| c.get("text").and_then(|t| t.as_str())).collect())
                .unwrap_or_default();
            return Err(anyhow!("MCP tool '{}' failed: {}", tool_name, if texts.is_empty() { result.to_string() } else { texts.join("\n") }));
        }
        Ok(result)
    }

    /// Synchronously retrieves cached or statically defined MCP tools without spawning a child process
    pub fn get_cached_tools(mcp_dir: &Path) -> Option<Vec<Value>> {
        let cache_key = mcp_dir.to_path_buf();
        if let Ok(cache) = MCP_TOOL_CACHE.try_read() {
            if let Some(tools) = cache.get(&cache_key) {
                return Some(tools.clone());
            }
        }
        if let Some(tools) = std::fs::read(tools_list_cache(mcp_dir))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Value>>(&bytes).ok())
        {
            return Some(tools);
        }
        let manifest_path = mcp_dir.join("package.json");
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                if let Some(tools_arr) = val.get("tools").and_then(|t| t.as_array()) {
                    if !tools_arr.is_empty() {
                        return Some(tools_arr.clone());
                    }
                }
            }
        }
        None
    }
}

/// The server's last tool list on disk (inside the package's cache folder, so a reinstall or
/// a full cache clear removes it)
fn tools_list_cache(mcp_dir: &Path) -> PathBuf {
    mcp_dir.join(".cache").join("tools_list.json")
}
