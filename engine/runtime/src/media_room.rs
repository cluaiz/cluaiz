//! Idle models of the other kinds (ONNX, GGUF speech, GGUF embedding) that a model may free when
//! memory is short for its load or its request. A model in use is never freed (each owner frees
//! only when its lock is free), and the chat model is never on this list.

use std::sync::{Arc, Mutex};

type FreeIdle = Box<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone, Default)]
pub struct MediaRoom(Arc<Mutex<Vec<(&'static str, FreeIdle)>>>);

impl MediaRoom {
    /// `free_idle` frees that owner's models when none is in use; true when one was freed
    pub(crate) fn register(&self, kind: &'static str, free_idle: impl Fn() -> bool + Send + Sync + 'static) {
        if let Ok(mut owners) = self.0.lock() {
            owners.push((kind, Box::new(free_idle)));
        }
    }

    /// Frees the idle models of every kind but `kind`; the kinds that freed one
    pub(crate) fn free_others(&self, kind: &str) -> Vec<&'static str> {
        let Ok(owners) = self.0.lock() else { return Vec::new() };
        owners
            .iter()
            .filter(|(owner, _)| *owner != kind)
            .filter_map(|(owner, free_idle)| free_idle().then_some(*owner))
            .collect()
    }

    /// Runs `attempt`; when it fails, frees the idle models of the kinds but `kind` and runs it
    /// once more (a memory check or a load that may have failed for lack of room)
    pub(crate) fn retry_freeing_others<T>(
        &self,
        kind: &str,
        mut attempt: impl FnMut() -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        attempt().or_else(|error| {
            let freed = self.free_others(kind);
            if freed.is_empty() {
                return Err(error);
            }
            tracing::info!("♻️ [{}] Freed idle {} model(s) after: {:#}", kind, freed.join(" + "), error);
            attempt()
        })
    }
}
