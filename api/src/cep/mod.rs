//! The Engine side of CEP (Cluaiz Engine Protocol): the call context every method gets, the
//! method registry, and the `POST /v1/cep` endpoint. The protocol itself (envelope, registry,
//! approvals, tasks, discovery) is the `cluaiz-cep` crate. Design: `.agent/plans/CEP/`.

pub mod http;
pub mod methods;
pub mod roots;

use crate::state::AppState;
use cluaiz_cep::{ApprovalStore, Engine, Limits, TaskStore};
use std::sync::Arc;
use std::time::Duration;

/// What a method gets besides its params: the Engine's state
#[derive(Clone)]
pub struct CallCtx {
    pub state: Arc<AppState>,
}

pub type CepEngine = Engine<CallCtx>;

/// How long the owner has to answer an approval request
const APPROVAL_TTL: Duration = Duration::from_secs(300);
/// Tasks: running at once per caller, kept after start by default, and at most
const MAX_RUNNING_TASKS: usize = 8;
const TASK_TTL_MS: u64 = 60 * 60 * 1000;
const MAX_TASK_TTL_MS: u64 = 24 * 60 * 60 * 1000;

pub fn engine() -> CepEngine {
    Engine::new(
        methods::registry(),
        ApprovalStore::new(APPROVAL_TTL),
        TaskStore::new(MAX_RUNNING_TASKS, TASK_TTL_MS, MAX_TASK_TTL_MS),
        Limits::default(),
        "Cluaiz Engine Protocol",
        env!("CARGO_PKG_VERSION"),
    )
    .with_audit(Arc::new(cluaiz_security::audit::AuditFile::in_home()))
}
