//! ═══════════════════════════════════════════════════════════════════════
//!   Shared: The Sovereign Reusable Core
//! ═══════════════════════════════════════════════════════════════════════
//!  This crate holds ALL business logic, data structures, and constants
//!  that are shared across every  interface:
//!  - CLI (Ratatui TUI)
//!  - API (Axum HTTP Gateway)
//!  - Desktop App (future)
//!  - Web App (future)
//!
//!  Rule: `shared` depends on NOTHING in the workspace.
//!        Everything depends on `shared`.
//! ═══════════════════════════════════════════════════════════════════════

pub mod profile;
pub mod auth;
