//! Runs Cluaiz Automation next to the Engine. The Engine does not depend on this crate: the
//! `cluaiz` program installs it before the server starts ([`install`]), and from then on it
//! serves the `flows.*` methods and reaches the Engine only through its public doors.

mod catalog;
mod engine;
mod methods;
mod reach;
mod service;
mod tools;
mod triggers;
mod webhook;

use cluaiz_api::cep::extensions::{Caller, Extension, ToolCall, ToolList};
use cluaiz_api::cep::CallCtx;
use cluaiz_api::AppState;
use cluaiz_cep::Registry;
use serde_json::{Map, Value};
use std::sync::Arc;

struct Automation;

impl Extension for Automation {
    fn register(&self, registry: &mut Registry<CallCtx>) {
        methods::register(registry);
    }

    fn start(&self, state: Arc<AppState>) {
        let service = service::get(&state);
        service.resume_all();
        triggers::listen(service);
    }

    fn routes(&self) -> Option<axum::Router<Arc<AppState>>> {
        Some(webhook::routes())
    }

    fn tools(&self, state: Arc<AppState>) -> Option<ToolList> {
        Some(Box::pin(tools::list(state)))
    }

    fn call_tool(&self, state: Arc<AppState>, name: &str, arguments: Map<String, Value>, caller: Caller) -> Option<ToolCall> {
        name.starts_with(tools::PREFIX).then(|| Box::pin(tools::call(state, name.to_string(), arguments, caller)) as ToolCall)
    }
}

/// Installs Automation into the server this process will start. Call once, before the server
/// boots; a second call does nothing.
pub fn install() {
    if !cluaiz_api::cep::extensions::install(vec![Arc::new(Automation)]) {
        tracing::warn!("Automation was already installed");
    }
}
