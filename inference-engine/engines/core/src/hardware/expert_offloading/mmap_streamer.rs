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
            use std::ffi::c_void;

            #[repr(C)]
            struct WIN32_MEMORY_RANGE_ENTRY {
                virtual_address: *mut c_void,
                number_of_bytes: usize,
            }

            extern "system" {
                fn GetCurrentProcess() -> *mut c_void;
                fn PrefetchVirtualMemory(
                    h_process: *mut c_void,
                    number_of_entries: usize,
                    virtual_addresses: *const WIN32_MEMORY_RANGE_ENTRY,
                    flags: u32,
                ) -> i32;
            }

            let base = self.mmap.as_ptr();
            let mmap_len = self.mmap.len();
            let mut entries = Vec::with_capacity(offsets.len());

            for entry in offsets {
                let gate_start = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length
                    + entry.up.byte_length
                    + entry.down.byte_length) as usize;

                if gate_start + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_start) } as *mut c_void;
                    entries.push(WIN32_MEMORY_RANGE_ENTRY {
                        virtual_address: ptr,
                        number_of_bytes: total_len,
                    });
                }
            }

            if !entries.is_empty() {
                unsafe {
                    let h_process = GetCurrentProcess();
                    PrefetchVirtualMemory(h_process, entries.len(), entries.as_ptr(), 0);
                }
            }
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
            use std::ffi::c_void;

            extern "system" {
                fn DiscardVirtualMemory(
                    virtual_address: *mut c_void,
                    size: usize,
                ) -> u32;
            }

            let base = self.mmap.as_ptr();
            let mmap_len = self.mmap.len();

            for entry in offsets {
                let gate_start = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length
                    + entry.up.byte_length
                    + entry.down.byte_length) as usize;

                if gate_start + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_start) } as *mut c_void;
                    unsafe {
                        DiscardVirtualMemory(ptr, total_len);
                    }
                }
            }
        }
    }

    /// Returns total model size on SSD in GB.
    pub fn size_gb(&self) -> f64 {
        (self.file_size_bytes as f64) / (1024.0 * 1024.0 * 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::expert_offloading::TensorRange;

    #[test]
    fn test_ssd_mmap_streamer() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_data.bin");

        let mut data = vec![0u8; 1000];
        for i in 0..1000 {
            data[i] = (i % 256) as u8;
        }
        std::fs::write(&file_path, &data).unwrap();

        let streamer_res = SsdMmapStreamer::open(&file_path);
        assert!(streamer_res.is_ok());
        let streamer = streamer_res.unwrap();

        assert_eq!(streamer.file_size_bytes, 1000);
        assert!(streamer.size_gb() > 0.0);

        let slice = streamer.pread_expert_slice(100, 50).unwrap();
        assert_eq!(slice.len(), 50);
        for i in 0..50 {
            assert_eq!(slice[i], ((100 + i) % 256) as u8);
        }

        assert!(streamer.pread_expert_slice(950, 100).is_err());

        let mock_offset = ExpertTensorOffset {
            layer: 0,
            expert_id: 0,
            gate: TensorRange { file_offset: 10, byte_length: 20 },
            up: TensorRange { file_offset: 40, byte_length: 30 },
            down: TensorRange { file_offset: 80, byte_length: 40 },
        };

        let index = ExpertOffsetIndex {
            offsets: vec![Some(mock_offset)],
            n_layers: 1,
            n_experts: 1,
        };

        let block = streamer.read_expert(&index, 0, 0).unwrap();
        assert_eq!(block.expert_id, 0);
        assert_eq!(block.layer_index, 0);
        assert_eq!(block.size_bytes, 90);
        assert_eq!(block.weights_data.len(), 90);

        let expected_gate = &data[10..30];
        let expected_up = &data[40..70];
        let expected_down = &data[80..120];

        assert_eq!(&block.weights_data[0..20], expected_gate);
        assert_eq!(&block.weights_data[20..50], expected_up);
        assert_eq!(&block.weights_data[50..90], expected_down);

        let index_entry = index.lookup(0, 0).unwrap();
        streamer.prefetch_experts(&[index_entry]);
        streamer.release_experts(&[index_entry]);

        let _ = std::fs::remove_file(&file_path);
    }
}


