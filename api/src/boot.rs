//! One start for every way the Engine runs: the HTTP / native-pipe daemon, and the in-process C
//! library (`cep_call`)

use crate::state::AppState;
use std::sync::Arc;

fn start(session_token: String) -> AppState {
    // Register model folders that are on disk but not in model_registry.json (as the server did before)
    cluaiz_models::registry::InstalledStateRegistry::sync_from_disk(&cluaiz_models::registry::ModelVault::root_dir());
    AppState::new(Arc::new(cluaiz_runtime::EngineHost::new()), session_token)
}

/// The daemon: a new session token, written to the home for local clients
pub fn boot() -> AppState {
    start(crate::auth::initialize_session_token())
}

/// Inside another program (`cep_call`): calls come from that process, never over the network, so
/// the home's session token is left alone (a daemon on the same home keeps working)
pub fn boot_in_process() -> AppState {
    start(format!("{}", uuid::Uuid::new_v4().simple()))
}
