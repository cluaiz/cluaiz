//! 🧠 Cognitive Storage Bridge: Trait abstraction for local and remote database engines.
//! This ensures cluaize is fully agnostic of where cluaizd is deployed.

use cluaize_shared::hardware::governor::HardwareGovernor;
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
    if let Ok(control) = HardwareGovernor::load_system_control() {
        if control.brain.is_enabled() {
            if control.brain.is_local() {
                // Option 2: Local Single Node using LMDB FFI directly
                tracing::info!("Initializing Local Database FFI Storage Bridge...");
                return Arc::new(super::local_bridge::LocalBridge::new());
            } else {
                // Option 1: Centralized Brain + Remote Engines
                let addr = &control.brain.cluaizd_connect_ffi;
                tracing::info!("Initializing Remote Database Network Storage Bridge targeting {}...", addr);
                return Arc::new(super::remote_bridge::RemoteBridge::new(addr));
            }
        }
    }
    tracing::info!("Database FFI is disabled. Initializing Fallback Storage Bridge.");
    Arc::new(FallbackBridge)
}
