//! Every CEP method of the Engine, one module per group. The protocol's own methods
//! (`rpc.discover`, `tasks.*`) come from the `cluaiz-cep` crate.

use super::CallCtx;
use cluaiz_cep::Registry;

pub mod approvals;
pub mod audio;
pub mod audit;
pub mod chat;
pub mod config;
pub mod context;
pub mod fs;
mod fs_picker;
pub mod kv;
pub mod memory;
pub mod models;
pub mod notes;
pub mod storage;
pub mod system;
pub mod system_exec;
pub mod tokens;
pub mod tools;
pub mod workspace;

pub fn registry() -> Registry<CallCtx> {
    let mut registry = Registry::new();
    approvals::register(&mut registry);
    audio::register(&mut registry);
    audit::register(&mut registry);
    chat::register(&mut registry);
    config::register(&mut registry);
    context::register(&mut registry);
    fs::register(&mut registry);
    kv::register(&mut registry);
    memory::register(&mut registry);
    models::register(&mut registry);
    notes::register(&mut registry);
    storage::register(&mut registry);
    system::register(&mut registry);
    system_exec::register(&mut registry);
    tokens::register(&mut registry);
    tools::register(&mut registry);
    workspace::register(&mut registry);
    registry
}
