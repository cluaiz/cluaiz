//! 🛡️ Cluaiz Security Subsystem (`security/`)
//! Contains filesystem jails, permission verification schemas, and audit guards.

pub mod permission_schema;
pub mod audit;

pub use permission_schema::{PermissionSchema, ApiAuth, SlotConfig, ModelSelection};
