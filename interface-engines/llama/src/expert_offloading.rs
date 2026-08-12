//! 🧠 GGUF MoE Expert Streaming Controller
//! Manages expert weight paging for MoE models running via llama.cpp.
//!
//! ## Core Constraint
//! Cluaiz cannot intercept the per-expert `pread()` call inside llama.cpp's forward pass
//! because llama.cpp owns the inference loop via FFI. We therefore use OS-level memory
//! management to guide which expert pages stay resident in RAM vs get paged to storage.
//!
//! ## Strategy
//! - `madvise(MADV_WILLNEED)` / `VirtualAlloc(MEM_COMMIT)`: hint OS to prefetch hot expert pages
//! - `madvise(MADV_DONTNEED)` / `VirtualFree(MEM_DECOMMIT)`: release cold expert pages from RAM
//!
//! This is the same OS-level windowing approach used by llama.cpp's own `--mmap` path.
//! We are not loading raw bytes ourselves — we're just influencing the OS page cache.
//!
//! ## Limitations
//! - Expert-level precision is limited because llama.cpp controls the actual tensor read.
//! - Effectiveness depends on whether llama.cpp uses mmap for this model (use_mmap=true).
//! - Best results on Linux (full madvise support). Windows: best-effort via VirtualAlloc.

use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use cluaiz_shared::hardware::expert_offloading::{
    ExpertOffsetIndex, MoeModelInfo, RoutingHeatTracker, SharedExpertCache,
};

// ─── Platform-specific memory advisory imports ────────────────────────────────

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

// ─── Controller ──────────────────────────────────────────────────────────────

/// Controls OS-level memory advisory hints for MoE expert weight pages.
///
/// Initialization steps (called once after model load):
/// 1. Build the `ExpertOffsetIndex` from the GGUF tensor table.
/// 2. Load `RoutingHeatTracker` to identify hot experts from previous sessions.
/// 3. Issue `MADV_WILLNEED` for all hot experts to pre-warm the OS page cache.
///
/// Per-inference steps (called once per token generation step):
/// 1. Receive the predicted active expert IDs from the routing decision.
/// 2. Issue `MADV_WILLNEED` for the next predicted experts (prefetch).
/// 3. Issue `MADV_DONTNEED` for the least-recently-used (cold) experts.
/// 4. Record routing decision in the heat tracker.
pub struct GgufMoeStreamingController {
    /// Path to the GGUF model file on disk.
    model_path: std::path::PathBuf,
    /// Expert offset index for byte-precise memory advisory calls.
    expert_index: ExpertOffsetIndex,
    /// Routing heat tracker — persists hot expert statistics across sessions.
    heat_tracker: Arc<Mutex<RoutingHeatTracker>>,
    /// LRU expert cache — tracks which experts are currently "warm" in OS page cache.
    cache: SharedExpertCache,
    /// Cross-platform memory mapping for advisory calls.
    mmap: Option<memmap2::Mmap>,
    /// MoE structural info.
    pub moe_info: MoeModelInfo,
    /// Records the expert IDs activated in the most recent routing step (for cold hints).
    last_active_experts: Vec<(usize, usize)>,
    /// Number of layers offloaded to GPU (VRAM) that should not be managed by the CPU streaming advisor.
    pub n_gpu_layers: usize,
    /// Authorized LRU Cache budget for dynamic eviction
    pub cache_budget_gb: f64,
}

// SAFETY: The mmap_base pointer is only used for madvise calls from a single thread at a time.
// GgufMoeStreamingController is always accessed behind an Arc<Mutex<>>.
unsafe impl Send for GgufMoeStreamingController {}
unsafe impl Sync for GgufMoeStreamingController {}

impl GgufMoeStreamingController {
    /// Initialize the controller for a loaded GGUF MoE model.
    pub fn new(
        model_path: &Path,
        moe_info: MoeModelInfo,
        cache_budget_gb: f64,
        n_gpu_layers: usize,
    ) -> anyhow::Result<Self> {
        info!(
            "🧠 [GgufMoeStreaming] Initializing for: {:?} | {} experts/layer | cache: {:.2}GB | GPU layers: {}",
            model_path.file_name().unwrap_or_default(),
            moe_info.expert_count,
            cache_budget_gb,
            n_gpu_layers
        );

        // Step 1: Build expert offset index
        let expert_index = ExpertOffsetIndex::from_gguf(model_path, moe_info.expert_count)
            .map_err(|e| anyhow::anyhow!("Failed to build expert index: {}", e))?;
        info!(
            "📖 [GgufMoeStreaming] Expert index built: {} entries indexed.",
            expert_index.indexed_count()
        );

        // Step 2: Load routing heat tracker
        let model_dir = model_path.parent().unwrap_or(Path::new("."));
        let heat_tracker = RoutingHeatTracker::new(
            moe_info.moe_layer_count,
            moe_info.expert_count,
            model_dir,
        );
        let heat_tracker = Arc::new(Mutex::new(heat_tracker));

        // Step 3: Set up LRU cache
        let cache = SharedExpertCache::new(cache_budget_gb);

        // Step 4: mmap the model file for advisory virtual memory calls
        let mmap = Self::try_mmap(model_path);

        let mut controller = Self {
            model_path: model_path.to_path_buf(),
            expert_index,
            heat_tracker,
            cache,
            mmap,
            moe_info,
            last_active_experts: Vec::new(),
            n_gpu_layers,
            cache_budget_gb,
        };

        // Step 5: Pre-warm OS page cache for hot experts from previous sessions
        controller.warm_hot_experts();

        Ok(controller)
    }

    /// Called once per inference step after the MoE router has selected experts.
    /// `layer`: current transformer layer index.
    /// `active_expert_ids`: the top-K expert IDs selected by the router for this token.
    /// `predicted_next_experts`: optional pre-predicted experts for next token (prefetch).
    pub fn on_routing_decision(
        &mut self,
        layer: usize,
        active_expert_ids: &[usize],
        predicted_next_experts: Option<&[(usize, usize)]>,
    ) {
        // 1. Record routing in heat tracker
        if let Ok(mut tracker) = self.heat_tracker.lock() {
            tracker.record_routing(layer, active_expert_ids);
        }

        // 2. Issue WILLNEED hints for currently active experts (ensure they stay in cache)
        for &expert_id in active_expert_ids {
            let key = (layer, expert_id);
            if let Some(pos) = self.last_active_experts.iter().position(|x| *x == key) {
                self.last_active_experts.remove(pos);
            }
            self.advise_willneed(layer, expert_id);
            self.last_active_experts.push(key);
        }

        // 3. Prefetch predicted next-step experts (read-ahead)
        if let Some(next_experts) = predicted_next_experts {
            for &(next_layer, next_expert) in next_experts {
                self.advise_willneed(next_layer, next_expert);
            }
        }

        // 4. Dynamic LRU pool eviction bounded by Negotiator's RAM Budget
        let max_experts_in_ram = if self.moe_info.expert_size_bytes > 0 {
            let max_bytes = self.cache_budget_gb * 1024.0 * 1024.0 * 1024.0;
            (max_bytes / self.moe_info.expert_size_bytes as f64) as usize
        } else {
            16
        };

        // Evict oldest experts if we exceed our calculated RAM budget
        while self.last_active_experts.len() > max_experts_in_ram {
            let (cold_layer, cold_expert) = self.last_active_experts.remove(0);
            self.advise_dontneed(cold_layer, cold_expert);
        }
    }

    // ── Private: OS advisory calls ────────────────────────────────────────────

    pub fn advise_willneed(&self, layer: usize, expert_id: usize) {
        if layer < self.n_gpu_layers {
            return;
        }
        #[cfg(unix)]
        {
            if let (Some(mmap), Some(entry)) = (self.mmap.as_ref(), self.expert_index.lookup(layer, expert_id)) {
                let base = mmap.as_ptr();
                let mmap_len = mmap.len();
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut libc::c_void;
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_WILLNEED);
                    }
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

            if let (Some(mmap), Some(entry)) = (self.mmap.as_ref(), self.expert_index.lookup(layer, expert_id)) {
                let base = mmap.as_ptr();
                let mmap_len = mmap.len();
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut c_void;
                    let mem_range = WIN32_MEMORY_RANGE_ENTRY {
                        virtual_address: ptr,
                        number_of_bytes: total_len,
                    };
                    unsafe {
                        let h_process = GetCurrentProcess();
                        PrefetchVirtualMemory(h_process, 1, &mem_range, 0);
                    }
                }
            }
        }
    }

    pub fn advise_dontneed(&self, layer: usize, expert_id: usize) {
        if layer < self.n_gpu_layers {
            return;
        }
        #[cfg(unix)]
        {
            if let (Some(mmap), Some(entry)) = (self.mmap.as_ref(), self.expert_index.lookup(layer, expert_id)) {
                let base = mmap.as_ptr();
                let mmap_len = mmap.len();
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut libc::c_void;
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
                fn VirtualUnlock(
                    lpAddress: *mut c_void,
                    dwSize: usize,
                ) -> i32;
            }

            if let (Some(mmap), Some(entry)) = (self.mmap.as_ref(), self.expert_index.lookup(layer, expert_id)) {
                let base = mmap.as_ptr();
                let mmap_len = mmap.len();
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut c_void;
                    unsafe {
                        DiscardVirtualMemory(ptr, total_len);
                        VirtualUnlock(ptr, total_len);
                    }
                }
            }
        }
    }

    /// 🌊 Immediately release memory for all experts across all layers EXCEPT `keep_layer`.
    /// Called right after model load to purge cold expert weight pages from physical RAM.
    pub fn purge_all_experts_except(&self, keep_layer: usize) {
        let total_layers = self.moe_info.moe_layer_count;
        let total_experts = self.moe_info.expert_count;
        info!(
            "🌊 [GgufMoeStreaming] Executing post-load memory purge across {} MoE layers (keeping layer {})...",
            total_layers, keep_layer
        );
        for layer in 0..total_layers {
            if layer == keep_layer {
                continue;
            }
            for expert_id in 0..total_experts {
                self.advise_dontneed(layer, expert_id);
            }
        }
        #[cfg(windows)]
        {
            extern "system" {
                fn GetCurrentProcess() -> *mut std::ffi::c_void;
                fn SetProcessWorkingSetSize(
                    hProcess: *mut std::ffi::c_void,
                    dwMinimumWorkingSetSize: usize,
                    dwMaximumWorkingSetSize: usize,
                ) -> i32;
            }
            unsafe {
                // Trim process working set to immediately return discardable expert pages to Windows free RAM
                SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
            }
        }
        info!("🌊 [GgufMoeStreaming] Post-load memory purge complete.");
    }

    /// Pre-warm top hot experts from heat tracker on startup.
    /// Bounded to max 16 hot experts to prevent NVMe SSD 100% Disk Queue choke on startup.
    pub fn warm_hot_experts(&mut self) {
        let first_cpu_layer = self.n_gpu_layers;
        if first_cpu_layer >= self.moe_info.moe_layer_count {
            return;
        }

        // Dynamically compute pre-warm limit based on model structure (Top-K activated experts per token)
        let max_prewarm_experts = (self.moe_info.active_experts_per_token * 2).max(8);
        let mut prewarm_count = 0;

        if let Ok(tracker) = self.heat_tracker.lock() {
            let mut layer_experts = Vec::new();
            for expert_id in 0..self.moe_info.expert_count {
                let frequency = tracker.get_expert_frequency(first_cpu_layer, expert_id);
                if frequency > 0 {
                    layer_experts.push((expert_id, frequency));
                }
            } 
            layer_experts.sort_by(|a, b| b.1.cmp(&a.1));
            
            info!(
                "🌡️ [GgufMoeStreaming] Pre-warming up to {} hot experts of first CPU layer (layer {}).",
                max_prewarm_experts, first_cpu_layer
            );

            for (expert_id, _) in layer_experts.into_iter().take(max_prewarm_experts) {
                self.advise_willneed(first_cpu_layer, expert_id);
                prewarm_count += 1;
            }
        }

        if prewarm_count == 0 {
            info!("🌡️ [GgufMoeStreaming] No prior routing heat data for layer {} — cold start.", first_cpu_layer);
        }
    }

    /// Try to memory-map the model file for advisory calls.
    /// Returns (Some(base_ptr), file_len) on success, (None, 0) on failure/Windows.
    fn try_mmap(model_path: &Path) -> Option<memmap2::Mmap> {
        use std::fs::File;
        let file = match File::open(model_path) {
            Ok(f) => f,
            Err(e) => {
                warn!("🧠 [GgufMoeStreaming] Cannot open model file for mmap: {}", e);
                return None;
            }
        };
        match unsafe { memmap2::Mmap::map(&file) } {
            Ok(mmap) => {
                info!(
                    "🧠 [GgufMoeStreaming] mmap established: {:.2} GB advisory window.",
                    mmap.len() as f64 / (1024.0 * 1024.0 * 1024.0)
                );
                Some(mmap)
            }
            Err(e) => {
                warn!("🧠 [GgufMoeStreaming] mmap failed — advisory hints disabled: {}", e);
                None
            }
        }
    }
}

impl Drop for GgufMoeStreamingController {
    fn drop(&mut self) {
        // Heat tracker auto-saves on drop via its own Drop impl
        info!("🧠 [GgufMoeStreaming] Controller dropped — heat data auto-saved.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_dummy_gguf(
        path: &std::path::Path,
        metadata_kvs: &[(&str, u32, &[u8])],
        tensors: &[(&str, &[u64])],
    ) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"GGUF").unwrap();
        file.write_all(&3u32.to_le_bytes()).unwrap();
        file.write_all(&(tensors.len() as u64).to_le_bytes()).unwrap();
        file.write_all(&(metadata_kvs.len() as u64).to_le_bytes()).unwrap();

        for (key, val_type, val_bytes) in metadata_kvs {
            file.write_all(&(key.len() as u64).to_le_bytes()).unwrap();
            file.write_all(key.as_bytes()).unwrap();
            file.write_all(&val_type.to_le_bytes()).unwrap();
            file.write_all(val_bytes).unwrap();
        }

        for (name, dims) in tensors {
            file.write_all(&(name.len() as u64).to_le_bytes()).unwrap();
            file.write_all(name.as_bytes()).unwrap();
            file.write_all(&(dims.len() as u32).to_le_bytes()).unwrap();
            for &dim in *dims {
                file.write_all(&dim.to_le_bytes()).unwrap();
            }
            file.write_all(&0u32.to_le_bytes()).unwrap();
            file.write_all(&0u64.to_le_bytes()).unwrap();
        }
    }

    #[test]
    fn test_gguf_moe_streaming_controller() {
        let temp_dir = std::env::temp_dir().join("gguf_moe_controller_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let model_path = temp_dir.join("model.gguf");

        let expert_count_bytes = 8u32.to_le_bytes();
        let metadata = vec![
            ("llm.expert_count", 4u32, &expert_count_bytes[..]),
        ];

        let tensors = vec![
            ("blk.0.ffn_gate_exps.weight", &[2048u64][..]),
            ("blk.0.ffn_up_exps.weight", &[2048u64][..]),
            ("blk.0.ffn_down_exps.weight", &[4096u64][..]),
            ("blk.1.ffn_gate_exps.weight", &[2048u64][..]),
            ("blk.1.ffn_up_exps.weight", &[2048u64][..]),
            ("blk.1.ffn_down_exps.weight", &[4096u64][..]),
        ];

        write_dummy_gguf(&model_path, &metadata, &tensors);

        let moe_info = MoeModelInfo {
            is_moe: true,
            expert_count: 8,
            moe_layer_count: 2,
            active_experts_per_token: 2,
            total_expert_bytes: 32768,
            expert_size_bytes: 2048,
            dense_backbone_bytes: 1024,
        };

        // 1. Initialize controller
        let controller_res = GgufMoeStreamingController::new(&model_path, moe_info, 0.001, 0);
        assert!(controller_res.is_ok());
        let mut controller = controller_res.unwrap();

        // 2. Perform routing decision
        // Token routing: layer 0, active experts: 2, 5. Prefetch for next step: layer 1, expert 3
        controller.on_routing_decision(0, &[2, 5], Some(&[(1, 3)]));

        // 3. Verify routing is recorded in heat tracker
        {
            let tracker = controller.heat_tracker.lock().unwrap();
            assert_eq!(tracker.get_count(0, 2), 1);
            assert_eq!(tracker.get_count(0, 5), 1);
            assert_eq!(tracker.get_count(0, 3), 0);
            assert_eq!(tracker.total_records(), 2);
        }

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

