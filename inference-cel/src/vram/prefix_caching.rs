//! Prefix Caching & Context Memory Validation Layer
//! Validates structured plugin, skill, and tool payloads for prefix caching and context continuation.

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct TensorData {
    pub dimensions: Vec<usize>,
    pub values: Vec<f32>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ContextInjectionEnvelope {
    pub tokens: Vec<u32>,        // Raw token IDs for prefix alignment
    pub sequence_id: u32,       // Sequence context safety
    pub tensor_data: TensorData, // Raw tensor floats
}

/// Validates CPU payload size and envelope deserialization before context submission
pub fn inject_from_cpu(cpu_payload: &[u8], target_layer: &str) -> Result<(), String> {
    // 1. Validate payload size against safety thresholds to prevent buffer overflow.
    const MAX_INJECTION_BYTES: usize = 300 * 1024 * 1024;
    if cpu_payload.len() > MAX_INJECTION_BYTES {
        return Err(format!("Buffer Spill Prevented: Payload size {} bytes exceeds the 300MB safe threshold.", cpu_payload.len()));
    }

    // 2. Deserialize binary envelope from Native/WASM plugins on host CPU memory
    let parsed_data: ContextInjectionEnvelope = bincode::deserialize(cpu_payload)
        .map_err(|e| format!("Failed to parse ContextInjectionEnvelope from Bincode payload: {}", e))?;
    
    tracing::info!("🔒 Prefix Context Validation Enforced: Processed {} tokens for sequence {} targeting: {}", 
        parsed_data.tokens.len(), parsed_data.sequence_id, target_layer);
    
    Ok(())
}
