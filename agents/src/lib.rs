//! 🤖 Cluaiz Agents Subsystem (`agents/`)
//! Contains MCP client, plugins runner, skills router, and execution sandboxes.

pub mod mcp;
pub mod plugins;
pub mod skills;
pub mod execution;
pub mod registry;
pub mod lifecycle;
pub mod installer;
pub mod prompt_compiler;
