//! 🧠 Cognitive Storage Bridge: Trait abstraction for local and remote database engines.
//! This ensures cluaiz is fully agnostic of where cluaizdb is deployed.

use cluaiz_shared::hardware::governor::HardwareGovernor;
use std::sync::Arc;

pub trait CognitiveStorageBridge: Send + Sync {
    /// 🧠 Direct Brain Injection: Pulls a Neuron payload from the database by key.
    fn inject_context(&self, memory_key: &str) -> Option<Vec<u8>>;

    /// ⚡ Direct Brain Write: Saves a Memory/Skill Vector directly to the database.
    fn save_context(&self, memory_id: &str, payload: &str, vector: &[f32]) -> Result<(), String>;
}

/// Fallback / Brain-off implementation of the Storage Bridge
pub struct FallbackBridge;

impl CognitiveStorageBridge for FallbackBridge {
    fn inject_context(&self, _memory_key: &str) -> Option<Vec<u8>> {
        tracing::debug!("FallbackBridge: FFI database connection is disabled.");
        None
    }

    fn save_context(&self, _memory_id: &str, _payload: &str, _vector: &[f32]) -> Result<(), String> {
        tracing::debug!("FallbackBridge: FFI database connection is disabled.");
        Ok(())
    }
}

/// Factory function to load the appropriate storage bridge based on system control configuration
pub fn load_storage_bridge() -> Arc<dyn CognitiveStorageBridge> {
    tracing::info!("Database FFI is disabled by default. Initializing Fallback Storage Bridge. Plugin will override.");
    Arc::new(FallbackBridge)
}
