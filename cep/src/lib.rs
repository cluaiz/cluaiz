//! CEP, the Cluaiz Engine Protocol: every Engine-specific operation as a JSON-RPC 2.0 method at
//! `/v1/cep`, in `cep_call`, and behind `/mcp`. This crate is the protocol only: envelope,
//! errors, method registry, principals, approvals, tasks, discovery and dispatch. It knows nothing
//! about HTTP, models or the Engine's state; the Engine registers its methods into a
//! `Registry<Ctx>` and hands requests to `Engine::call`.
//!
//! Design: `.agent/plans/CEP/01-architecture.md`.

pub mod approval;
pub mod discover;
pub mod dispatch;
pub mod envelope;
pub mod error;
pub mod principal;
mod protocol;
pub mod registry;
pub mod task;

pub use approval::ApprovalStore;
pub use dispatch::{Audit, AuditEntry, Engine, Limits};
pub use error::CepError;
pub use principal::{OnDenied, Principal, Scope};
pub use registry::{Call, Execution, Meta, NoParams, Registry};
pub use task::{Progress, TaskHandle, TaskStore};
