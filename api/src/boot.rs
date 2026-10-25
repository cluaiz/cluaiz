//! One start for every way the Engine runs: the HTTP / native-pipe daemon now, the in-process
//! C library (`cep_call`) later (`.agent/plans/CEP/04-implementation-plan.md` P1.2)

use crate::state::AppState;
use std::sync::Arc;

/// The Engine's state, ready to serve
pub fn boot() -> AppState {
    // Register model folders that are on disk but not in model_registry.json (as the server did before)
    cluaiz_models::registry::InstalledStateRegistry::sync_from_disk(&cluaiz_models::registry::ModelVault::root_dir());
    AppState::new(Arc::new(cluaiz_runtime::EngineHost::new()))
}
