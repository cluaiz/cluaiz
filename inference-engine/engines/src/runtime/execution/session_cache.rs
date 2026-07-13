use dashmap::DashMap;
use once_cell::sync::Lazy;
use cluaiz_shared::hardware::memory::kv_cache::stitching::cluaizSignal;
use uuid::Uuid;

/// 🧠 CoreSessionCache: The Persistent Memory of the cluaiz OS.
/// Stores KV cache signals indexed by session ID to prevent instruction forgetting.
pub static SESSION_CACHE: Lazy<DashMap<Uuid, cluaizSignal>> = Lazy::new(DashMap::new);

pub struct SessionManager;

impl SessionManager {
    /// 🔗 Stitch Signal: Saves the current Core state for the given session.
    pub fn stitch(session_id: Uuid, signal: cluaizSignal) {
        SESSION_CACHE.insert(session_id, signal);
        tracing::debug!("🧬 [Session] Core signal stitched for session: {}", session_id);
    }

    /// 🧬 Recall Signal: Retrieves the Core state for the given session.
    pub fn recall(session_id: &Uuid) -> Option<cluaizSignal> {
        SESSION_CACHE.get(session_id).map(|s| s.clone())
    }

    /// 🧹 Purge: Clears memory for a specific session.
    pub fn purge(session_id: &Uuid) {
        SESSION_CACHE.remove(session_id);
    }
}
