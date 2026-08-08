//! 💾 Zero-Copy SSD Memory Mapping Streamer
//! Colibri Architecture: Zero-copy mmap streamer for disk-backed MoE expert loading.
//! Allows reading tensor slices directly from high-speed NVMe SSD files on demand
//! during live token generation passes.

use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::hardware::expert_offloading::{ExpertOffsetIndex, ExpertTensorOffset, LoadedExpertBlock};

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

    /// Reads a specific expert's gate + up + down weight tensors from disk.
    /// Returns a `LoadedExpertBlock` with all tensor bytes copied into an Arc<Vec<u8>>.
    ///
    /// Uses the `ExpertOffsetIndex` for byte-precise reads — mirrors Colibri's `pread_expert()`.
    pub fn read_expert(
        &self,
        index: &ExpertOffsetIndex,
        layer: usize,
        expert_id: usize,
    ) -> anyhow::Result<LoadedExpertBlock> {
        let entry = index
            .lookup(layer, expert_id)
            .ok_or_else(|| anyhow::anyhow!("Expert L{}E{} not found in offset index", layer, expert_id))?;

        let gate_start = entry.gate.file_offset as usize;
        let gate_end = gate_start + entry.gate.byte_length as usize;
        let up_start = entry.up.file_offset as usize;
        let up_end = up_start + entry.up.byte_length as usize;
        let down_start = entry.down.file_offset as usize;
        let down_end = down_start + entry.down.byte_length as usize;

        let mmap_len = self.mmap.len();
        if gate_end > mmap_len || up_end > mmap_len || down_end > mmap_len {
            anyhow::bail!(
                "Expert L{}E{} offsets exceed file size ({} bytes)",
                layer, expert_id, mmap_len
            );
        }

        // Zero-copy: concatenate gate + up + down into a single owned buffer
        let total_bytes = entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length;
        let mut weights_data = Vec::with_capacity(total_bytes as usize);
        weights_data.extend_from_slice(&self.mmap[gate_start..gate_end]);
        weights_data.extend_from_slice(&self.mmap[up_start..up_end]);
        weights_data.extend_from_slice(&self.mmap[down_start..down_end]);

        Ok(LoadedExpertBlock {
            expert_id,
            layer_index: layer,
            size_bytes: total_bytes as usize,
            weights_data: Arc::new(weights_data),
        })
    }

    /// Issues `MADV_WILLNEED` hints for a list of expert tensor ranges.
    /// Tells the OS to prefetch these pages into the page cache before they are needed.
    /// No-op on Windows (OS handles prefetch automatically via mmap read-ahead).
    pub fn prefetch_experts(&self, offsets: &[&ExpertTensorOffset]) {
        #[cfg(unix)]
        {
            for entry in offsets {
                let base = self.mmap.as_ptr() as *mut libc::c_void;
                let mmap_len = self.mmap.len();

                let gate_start = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length
                    + entry.up.byte_length
                    + entry.down.byte_length) as usize;

                if gate_start + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_start) };
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_WILLNEED);
                    }
                } else {
                    warn!(
                        "⚠️ [SSD-Streamer] Prefetch L{}E{}: offset range exceeds file size, skipping.",
                        entry.layer, entry.expert_id
                    );
                }
            }
        }
        #[cfg(windows)]
        {
            // Windows: PrefetchVirtualMemory could be used here but requires Win8.1+
            // and the `windows` crate. For now, rely on OS read-ahead via mmap access pattern.
            let _ = offsets; // suppress unused warning
        }
    }

    /// Issues `MADV_DONTNEED` hints to release cold expert pages from the OS page cache.
    /// Frees physical RAM used by cold expert weights without unmapping the file.
    pub fn release_experts(&self, offsets: &[&ExpertTensorOffset]) {
        #[cfg(unix)]
        {
            for entry in offsets {
                let base = self.mmap.as_ptr() as *mut libc::c_void;
                let mmap_len = self.mmap.len();

                let gate_start = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length
                    + entry.up.byte_length
                    + entry.down.byte_length) as usize;

                if gate_start + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_start) };
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_DONTNEED);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = offsets;
        }
    }

    /// Returns total model size on SSD in GB.
    pub fn size_gb(&self) -> f64 {
        (self.file_size_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
    }
}

