//! 💾 Zero-Copy SSD Memory Mapping Streamer
//! Colibri Architecture: Zero-copy mmap streamer for disk-backed MoE expert loading.
//! Allows reading tensor slices directly from high-speed NVMe SSD files on demand
//! during live token generation passes.

use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, error};

/// Zero-copy SSD streamer wrapping a memory-mapped model file.
pub struct SsdMmapStreamer {
    file_path: std::path::PathBuf,
    mmap: Arc<Mmap>,
    file_size_bytes: u64,
}

impl SsdMmapStreamer {
    /// Opens and memory-maps a model file on disk.
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let file_size_bytes = file.metadata()?.len();

        let mmap = unsafe { Mmap::map(&file)? };
        
        // Advise OS kernel to optimize for random access reading of expert weights
        #[cfg(unix)]
        unsafe {
            libc::madvise(
                mmap.as_ptr() as *mut std::ffi::c_void,
                mmap.len(),
                libc::MADV_RANDOM,
            );
        }

        info!(
            "💾 [SSD-Streamer] Memory-mapped file {:?} | Size: {:.2} GB",
            p.file_name().unwrap_or_default(),
            (file_size_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
        );

        Ok(Self {
            file_path: p.to_path_buf(),
            mmap: Arc::new(mmap),
            file_size_bytes,
        })
    }

    /// Reads an expert byte slice from disk using zero-copy mmap offset bounds.
    pub fn pread_expert_slice(&self, offset: usize, length: usize) -> anyhow::Result<&[u8]> {
        if offset + length > self.mmap.len() {
            error!(
                "❌ [SSD-Streamer] Read out of bounds: offset {} + len {} > file len {}",
                offset, length, self.mmap.len()
            );
            anyhow::bail!("SSD read out of bounds");
        }

        Ok(&self.mmap[offset..offset + length])
    }

    /// Returns total model size on SSD in GB.
    pub fn size_gb(&self) -> f64 {
        (self.file_size_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
    }
}
