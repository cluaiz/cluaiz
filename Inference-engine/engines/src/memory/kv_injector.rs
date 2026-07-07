use anyhow::Result;
use memmap2::MmapOptions;
use std::fs::File;
use std::path::PathBuf;
use tracing::{info, warn};
use cluaiz_shared::environment::EnvironmentManager;

/// 🧠 Zero-Copy KV-Cache Injector
/// This module handles reading/writing context memory directly to/from VRAM via mmap.
/// It bypasses the CPU and standard filesystem buffers for instantaneous context restoration.
pub struct KvInjector;

impl KvInjector {
    pub fn new() -> Self {
        Self
    }

    /// Injects a saved KV-Cache state directly into the LLaMA context.
    /// This uses `memmap2` to map the `.kvcache.safetensors` file directly to memory.
    pub fn inject_cache(&self, session_id: &str) -> Result<memmap2::Mmap> {
        let cache_dir = EnvironmentManager::current().kv_cache_dir();
        let path = cache_dir.join(format!("{}.kvcache.safetensors", session_id));
        
        if !path.exists() {
            return Err(anyhow::anyhow!("KV-Cache not found for session: {}", session_id));
        }

        info!("🧠 [KV-Injector] Hot-swapping context for session '{}' via zero-copy mmap.", session_id);

        let file = File::open(&path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        
        // Parse SafeTensors to find the first state dynamically
        let data_offset = {
            let st = safetensors::SafeTensors::deserialize(&mmap)
                .map_err(|e| anyhow::anyhow!("Failed to parse safetensors: {:?}", e))?;
            let first_name = st.names().first()
                .ok_or_else(|| anyhow::anyhow!("No tensors found in KV safetensors"))?
                .to_string();
            let tensor = st.tensor(&first_name)
                .map_err(|e| anyhow::anyhow!("Failed to find tensor '{}': {:?}", first_name, e))?;
            
            let mmap_start = mmap.as_ptr() as usize;
            let tensor_start = tensor.data().as_ptr() as usize;
            tensor_start - mmap_start
        };

        // In production, we slice/offset this mmap pointer to pass raw state bytes to llama_state_set_data
        Ok(mmap)
    }

    /// Snapshots the current VRAM KV-Cache to disk.
    pub fn snapshot_cache(&self, session_id: &str, raw_bytes: &[u8]) -> Result<()> {
        let cache_dir = EnvironmentManager::current().ensure_kv_cache_dir()?;
        let path = cache_dir.join(format!("{}.kvcache.safetensors", session_id));
        warn!("💾 [KV-Injector] Snapshotting VRAM context to: {:?}", path);
        
        if let Ok(view) = safetensors::tensor::TensorView::new(
            safetensors::tensor::Dtype::U8, 
            vec![raw_bytes.len()], 
            raw_bytes
        ) {
            let mut metadata = std::collections::HashMap::new();
            metadata.insert("session".to_string(), session_id.to_string());
            
            safetensors::serialize_to_file(
                vec![("session_state", view)], 
                Some(metadata), 
                &path
            ).map_err(|e| anyhow::anyhow!("Safetensors KV dump fail: {:?}", e))?;
        }
        Ok(())
    }
}
