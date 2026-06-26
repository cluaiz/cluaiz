//! Memory page size constant has moved to `wasm_sandbox.rs` where it is actually used.
//! See `execution::wasm_sandbox::WASM_PAGE_SIZE`.
//!
//! This module is retained as a placeholder for future memory boundary utilities
//! (e.g., `max_memory_mb` enforcement helpers, WASM linear memory validators).
//! If no utilities are added in future, this file will be deleted per CERD LAW 8.
