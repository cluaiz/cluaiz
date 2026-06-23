//! 🏠 Local Bridge: Local memory-mapped LMDB database FFI bridge.
//! Leverages the zero-copy virtual address space mapping of TensorTransducer.

use super::storage_bridge::CognitiveStorageBridge;
use super::tensor_transducer::TensorTransducer;

pub struct LocalBridge;

impl LocalBridge {
    pub fn new() -> Self {
        // Ensure the environment is booted
        TensorTransducer::boot_environment();
        LocalBridge
    }
}

impl CognitiveStorageBridge for LocalBridge {
    fn inject_context(&self, memory_key: &str) -> Option<Vec<u8>> {
        TensorTransducer::inject_context(memory_key)
    }

    fn save_context(&self, memory_id: &str, payload: &str, vector: &[f32]) -> Result<(), String> {
        TensorTransducer::save_context(memory_id, payload, vector)
    }
}
