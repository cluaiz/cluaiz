//! Layers outside the Engine that serve their own CEP methods at `/v1/cep` (Automation). The Engine
//! does not know them: the program that starts the server installs them, and they reach the Engine
//! only through its public doors, like any client.

use super::CallCtx;
use crate::state::AppState;
use cluaiz_cep::{CepError, Registry};
use serde_json::{Map, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

/// The tools an extension offers, as MCP `tools/list` items. `_meta["cluaiz/confirm"]` (a
/// boolean) says whether a call asks the owner first.
pub type ToolList = Pin<Box<dyn Future<Output = Vec<Value>> + Send>>;
/// One call of an extension's tool: its result, or an error the client's model reads
pub type ToolCall = Pin<Box<dyn Future<Output = Result<Value, CepError>> + Send>>;

/// Who calls an extension's tool
#[derive(Debug, Clone)]
pub struct Caller {
    /// `mcp`, `chat` or `agent`
    pub kind: &'static str,
    /// The chat, for a call from a chat
    pub chat_id: Option<String>,
    /// The owner already said yes to this call (a chat's approval card): the tool does not ask again
    pub confirmed: bool,
}

/// `_meta` key of a listed tool: a call asks the owner first
pub const CONFIRM_META: &str = "cluaiz/confirm";

pub trait Extension: Send + Sync {
    /// Adds the extension's method group(s) to the registry, before discovery is built
    fn register(&self, registry: &mut Registry<CallCtx>);

    /// Called once the server's state exists, before it serves requests
    fn start(&self, _state: Arc<AppState>) {}

    /// HTTP routes of the extension for callers that cannot speak JSON-RPC (webhooks from outside
    /// services). They are served without the API token: each route proves its callers itself.
    fn routes(&self) -> Option<axum::Router<Arc<AppState>>> {
        None
    }

    /// Tools the extension offers to MCP clients (`POST /mcp`, next to the Engine's methods) and
    /// to chats, such as Automation's flows. Their names must not be CEP method names.
    fn tools(&self, _state: Arc<AppState>) -> Option<ToolList> {
        None
    }

    /// Runs one of the extension's tools; `None` when `name` is not one of them
    fn call_tool(
        &self,
        _state: Arc<AppState>,
        _name: &str,
        _arguments: Map<String, Value>,
        _caller: Caller,
    ) -> Option<ToolCall> {
        None
    }
}

static EXTENSIONS: OnceLock<Vec<Arc<dyn Extension>>> = OnceLock::new();

/// Installs the extensions; once per process, before the server boots. `false` when they were
/// already installed.
pub fn install(extensions: Vec<Arc<dyn Extension>>) -> bool {
    EXTENSIONS.set(extensions).is_ok()
}

pub(crate) fn installed() -> &'static [Arc<dyn Extension>] {
    EXTENSIONS.get().map(Vec::as_slice).unwrap_or_default()
}

/// The routes of every installed extension, as one router
pub(crate) fn routes() -> axum::Router<Arc<AppState>> {
    installed()
        .iter()
        .filter_map(|e| e.routes())
        .fold(axum::Router::new(), axum::Router::merge)
}

/// The tools of every installed extension
pub async fn tools(state: &Arc<AppState>) -> Vec<Value> {
    let mut list = Vec::new();
    for extension in installed() {
        if let Some(more) = extension.tools(state.clone()) {
            list.extend(more.await);
        }
    }
    list
}

/// A call of an extension's tool; `None` when no extension has a tool of that name
pub fn call_tool(
    state: &Arc<AppState>,
    name: &str,
    arguments: Map<String, Value>,
    caller: Caller,
) -> Option<ToolCall> {
    installed()
        .iter()
        .find_map(|e| e.call_tool(state.clone(), name, arguments.clone(), caller.clone()))
}

/// Starts every installed extension with the server's state
pub fn start(state: &Arc<AppState>) {
    for extension in installed() {
        extension.start(state.clone());
    }
}
