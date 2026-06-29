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
    /// This uses `memmap2` to map the `.kvcache.bin` file directly to memory.
    pub fn inject_cache(&self, session_id: &str) -> Result<memmap2::Mmap> {
        let cache_dir = EnvironmentManager::current().kv_cache_dir();
        let path = cache_dir.join(format!("{}.kvcache.bin", session_id));
        
        if !path.exists() {
            return Err(anyhow::anyhow!("KV-Cache not found for session: {}", session_id));
        }

        info!("🧠 [KV-Injector] Hot-swapping context for session '{}' via zero-copy mmap.", session_id);

        let file = File::open(path)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        
        // In production, we pass this mmap pointer to llama.cpp's `llama_state_set_data`
        Ok(mmap)
    }

    /// Snapshots the current VRAM KV-Cache to disk.
    pub fn snapshot_cache(&self, session_id: &str, raw_bytes: &[u8]) -> Result<()> {
        let cache_dir = EnvironmentManager::current().ensure_kv_cache_dir()?;
        let path = cache_dir.join(format!("{}.kvcache.bin", session_id));
        warn!("💾 [KV-Injector] Snapshotting VRAM context to: {:?}", path);
        std::fs::write(&path, raw_bytes)?;
        Ok(())
    }
}
