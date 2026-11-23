//! 🛡️ Cluaiz Security Subsystem (`security/`)
//! The `permission.json` schema and the CEP audit log file. Enforcement lives at each call site
//! (see README.md).

pub mod permission_schema;
pub mod audit;

pub use permission_schema::{PermissionSchema, ApiAuth, ExecPolicy, SlotConfig, ModelSelection, SecurityModeInfo, OS_SANDBOX_MODES, SECURITY_MODES, UNUSED_FIELDS, chat_security_mode};
