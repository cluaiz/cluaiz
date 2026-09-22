use dashmap::DashMap;
use once_cell::sync::Lazy;
use engine_core::hardware::memory::kv_cache::stitching::KernelSignal;
use uuid::Uuid;

/// 🧠 CoreSessionCache: The Persistent Memory of the cluaiz OS.
/// Stores KV cache signals indexed by session ID to prevent instruction forgetting.
pub static SESSION_CACHE: Lazy<DashMap<Uuid, KernelSignal>> = Lazy::new(DashMap::new);

pub struct SessionManager;

impl SessionManager {
    /// 🔗 Stitch Signal: Saves the current Core state for the given session.
    pub fn stitch(session_id: Uuid, signal: KernelSignal) {
        SESSION_CACHE.insert(session_id, signal);
        tracing::debug!("🧬 [Session] Core signal stitched for session: {}", session_id);
    }

    /// 🧬 Recall Signal: Retrieves the Core state for the given session.
    pub fn recall(session_id: &Uuid) -> Option<KernelSignal> {
        SESSION_CACHE.get(session_id).map(|s| s.clone())
    }

    /// 🧹 Purge: Clears memory for a specific session.
    pub fn purge(session_id: &Uuid) {
        SESSION_CACHE.remove(session_id);
    }
}
