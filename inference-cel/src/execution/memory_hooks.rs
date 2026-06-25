//! Memory hooks for host interaction.
//! In the DB (genome), these are exposed directly to the `WasmExecutor`.

/// Utility definitions for allocating and deallocating memory inside the WASM boundary.
/// (These are mostly reference implementations for what the plugin side should use).
pub const WASM_PAGE_SIZE: usize = 65536;
