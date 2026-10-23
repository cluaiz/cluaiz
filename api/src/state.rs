//! 🏛️ Global Application State (`api/src/state.rs`)
//! Central unified runtime context for Cluaiz API gateway.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use cluaiz_runtime::EngineHost;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingPermissionRequest {
    pub id: String,
    pub action: String,       // "fs_read", "fs_write", "fs_delete", "execute_cmd"
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

#[derive(Clone)]
pub struct AppState {
    pub host: Arc<EngineHost>,
    pub pending_permissions: Arc<RwLock<HashMap<String, PendingPermissionRequest>>>,
    pub active_approvals: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<ApprovalDecision>>>>,
    pub current_workspace_dir: Arc<RwLock<PathBuf>>,
    pub session_token: Arc<String>,
    pub permission_schema: Arc<RwLock<cluaiz_security::PermissionSchema>>,
}

impl AppState {
    pub fn new(host: Arc<EngineHost>) -> Self {
        let workspace = cluaiz_core::EnvironmentManager::current().local_dir.clone();
        cluaiz_agents::ToolsEngine::set_workspace_dir(workspace.clone());
        Self {
            host,
            pending_permissions: Arc::new(RwLock::new(HashMap::new())),
            active_approvals: Arc::new(RwLock::new(HashMap::new())),
            current_workspace_dir: Arc::new(RwLock::new(workspace)),
            session_token: Arc::new(crate::auth::initialize_session_token()),
            permission_schema: Arc::new(RwLock::new(cluaiz_security::PermissionSchema::load())),
        }
    }

    pub fn new_test_state(workspace: PathBuf) -> Self {
        Self {
            host: Arc::new(EngineHost::new()),
            pending_permissions: Arc::new(RwLock::new(HashMap::new())),
            active_approvals: Arc::new(RwLock::new(HashMap::new())),
            current_workspace_dir: Arc::new(RwLock::new(workspace)),
            session_token: Arc::new("test_session_token".to_string()),
            permission_schema: Arc::new(RwLock::new(cluaiz_security::PermissionSchema::default())),
        }
    }
}
