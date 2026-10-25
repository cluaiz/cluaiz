//! 🏛️ Global Application State (`api/src/state.rs`)
//! Central unified runtime context for Cluaiz API gateway.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use cluaiz_runtime::EngineHost;

#[derive(Clone)]
pub struct AppState {
    pub host: Arc<EngineHost>,
    pub current_workspace_dir: Arc<RwLock<PathBuf>>,
    pub session_token: Arc<String>,
    pub permission_schema: Arc<RwLock<cluaiz_security::PermissionSchema>>,
    /// CEP: the method registry, approvals and tasks behind `/v1/cep`
    pub cep: Arc<crate::cep::CepEngine>,
}

impl AppState {
    pub fn new(host: Arc<EngineHost>) -> Self {
        let workspace = cluaiz_core::EnvironmentManager::current().local_dir.clone();
        cluaiz_agents::ToolsEngine::set_workspace_dir(workspace.clone());
        Self {
            host,
            current_workspace_dir: Arc::new(RwLock::new(workspace)),
            session_token: Arc::new(crate::auth::initialize_session_token()),
            permission_schema: Arc::new(RwLock::new(cluaiz_security::PermissionSchema::load())),
            cep: Arc::new(crate::cep::engine()),
        }
    }

    pub fn new_test_state(workspace: PathBuf) -> Self {
        Self {
            host: Arc::new(EngineHost::new()),
            current_workspace_dir: Arc::new(RwLock::new(workspace)),
            session_token: Arc::new("test_session_token".to_string()),
            permission_schema: Arc::new(RwLock::new(cluaiz_security::PermissionSchema::default())),
            cep: Arc::new(crate::cep::engine()),
        }
    }
}
