use dispatcher::NeuralDispatcher;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPermissionRequest {
    pub id: String,
    pub action: String,       // e.g. "fs_read", "fs_write", "fs_delete", "execute_cmd"
    pub target: String,       // target path or command
    pub caller: String,       // "agent"
    pub timestamp_ms: u64,
    pub status: String,       // "pending", "approved", "rejected"
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub feedback: Option<String>,
}

/// Shared application state containing the dispatcher.
pub struct AppState {
    pub dispatcher: NeuralDispatcher,
    pub embedding_dispatcher: std::sync::Arc<dispatcher::EmbeddingDispatcher>,
    pub pending_permissions: Arc<RwLock<HashMap<String, PendingPermissionRequest>>>,
    pub active_approvals: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>>>,
    pub current_workspace_dir: Arc<RwLock<PathBuf>>,
    pub session_token: Arc<String>,
}

impl AppState {
    pub fn new_test_state(workspace: PathBuf) -> Arc<Self> {
        let dispatcher = NeuralDispatcher::new(Default::default(), Default::default());
        let embedding_dispatcher = Arc::new(dispatcher::EmbeddingDispatcher::new(None).unwrap());
        Arc::new(Self {
            dispatcher,
            embedding_dispatcher,
            pending_permissions: Arc::new(RwLock::new(HashMap::new())),
            active_approvals: Arc::new(RwLock::new(HashMap::new())),
            current_workspace_dir: Arc::new(RwLock::new(workspace)),
            session_token: Arc::new("test_secret_token_abcdef123".to_string()),
        })
    }
}
