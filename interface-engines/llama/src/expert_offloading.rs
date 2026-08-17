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
    AsyncExpertPrefetcher, DirectFileReader, ExpertOffsetIndex, MoeModelInfo, RoutingHeatTracker,
    SharedExpertCache, SharedStagingBuffer,
};

// ─── Platform-specific memory advisory imports ────────────────────────────────

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

// ─── Controller ──────────────────────────────────────────────────────────────

/// Controls OS-level memory advisory hints and Direct I/O prefetch workers for MoE expert weight pages.
///
/// Initialization steps (called once after model load):
/// 1. Build the `ExpertOffsetIndex` from the GGUF tensor table.
/// 2. Load `RoutingHeatTracker` to identify hot experts from previous sessions.
/// 3. Lock/Pin hottest experts in RAM pool (80/20 power law).
/// 4. Spawn dedicated `AsyncExpertPrefetcher` background worker for Direct I/O streaming.
///
/// Per-inference steps (called once per token generation step):
/// 1. Receive the predicted active expert IDs from the routing decision.
/// 2. Issue async Direct I/O prefetch request for upcoming layer $N+1$.
/// 3. Issue OS `WILLNEED` hints and record routing decision in the heat tracker.
pub struct GgufMoeStreamingController {
    /// Path to the GGUF model file on disk.
    model_path: std::path::PathBuf,
    /// Expert offset index for byte-precise memory advisory calls.
    pub expert_index: Arc<ExpertOffsetIndex>,
    /// Routing heat tracker — persists hot expert statistics across sessions.
    heat_tracker: Arc<Mutex<RoutingHeatTracker>>,
    /// LRU expert cache — tracks which experts are currently "warm" in OS page cache.
    pub cache: SharedExpertCache,
    /// Asynchronous background worker for Direct I/O streaming.
    pub async_prefetcher: Option<Arc<AsyncExpertPrefetcher>>,
    /// Cross-platform memory mapping for advisory calls.
    mmap: Option<memmap2::Mmap>,
    /// MoE structural info.
    pub moe_info: MoeModelInfo,
    /// Records the expert IDs activated in the most recent routing step (for cold hints).
    last_active_experts: Vec<(usize, usize)>,
    /// Permanently pinned hot experts across sessions.
    pub pinned_hot_experts: Vec<(usize, usize)>,
    /// Number of layers offloaded to GPU (VRAM) that should not be managed by the CPU streaming advisor.
    pub n_gpu_layers: usize,
    /// Authorized LRU Cache budget for dynamic eviction
    pub cache_budget_gb: f64,
    /// True CUDA Host PCIe DMA Streamer for streaming active experts into GPU VRAM
    pub dma_streamer: Option<Arc<crate::cuda_dma_streamer::CudaDmaStreamer>>,
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
        let expert_index_arc = Arc::new(expert_index);

        // Step 2: Load routing heat tracker
        let model_dir = model_path.parent().unwrap_or(Path::new("."));
        let heat_tracker = RoutingHeatTracker::new(
            moe_info.moe_layer_count,
            moe_info.expert_count,
            model_dir,
        );

        // Step 3: Identify hottest experts for permanent pinning (80/20 rule)
        let pin_budget_bytes = ((cache_budget_gb * 0.30) * 1024.0 * 1024.0 * 1024.0) as u64;
        let pinned_hot_experts = heat_tracker.get_hottest_experts(
            pin_budget_bytes,
            moe_info.expert_size_bytes as u64,
        );
        if !pinned_hot_experts.is_empty() {
            info!(
                "📌 [GgufMoeStreaming] Pinning {} hot experts in physical RAM cache.",
                pinned_hot_experts.len()
            );
        }
        let heat_tracker = Arc::new(Mutex::new(heat_tracker));

        // Step 4: Set up LRU cache
        let cache = SharedExpertCache::new(cache_budget_gb);

        // Step 5: Spawn dedicated Direct I/O Async Prefetcher Worker
        let async_prefetcher = match AsyncExpertPrefetcher::spawn(
            model_path,
            Arc::clone(&expert_index_arc),
            cache.clone(),
            64 * 1024 * 1024,
        ) {
            Ok(prefetcher) => {
                info!("⚡ [GgufMoeStreaming] Direct I/O Async Prefetcher spawned successfully.");
                Some(Arc::new(prefetcher))
            }
            Err(e) => {
                warn!("⚠️ [GgufMoeStreaming] Could not spawn Direct I/O worker (falling back to OS mmap): {}", e);
                None
            }
        };

        // Step 6: mmap the model file for advisory virtual memory calls
        let mmap = Self::try_mmap(model_path);

        // Step 6.5: CudaDmaStreamer is DEFERRED — initialized after model load
        // so it sees real post-load free VRAM (~250 MB) instead of pre-load (~2676 MB).
        // This prevents locking ~2.70 GB of pinned host RAM before model even loads.
        let dma_streamer = None;

        let mut controller = Self {
            model_path: model_path.to_path_buf(),
            expert_index: expert_index_arc,
            heat_tracker,
            cache,
            async_prefetcher,
            mmap,
            moe_info,
            last_active_experts: Vec::new(),
            pinned_hot_experts,
            n_gpu_layers,
            cache_budget_gb,
            dma_streamer,
        };

        // Step 7: Pre-warm OS page cache for hot experts from previous sessions
        controller.warm_hot_experts();

        Ok(controller)
    }

    /// 🚀 Deferred DMA Streamer Initialization — MUST be called AFTER model load.
    /// This ensures CudaDmaStreamer sees real post-load free VRAM and allocates
    /// appropriately sized pinned host buffers (~204 MB instead of ~2.70 GB).
    pub fn init_dma_streamer(&mut self) {
        let single_layer_expert_chunk = (self.moe_info.expert_size_bytes * (self.moe_info.active_experts_per_token as u64))
            .try_into()
            .unwrap_or(32 * 1024 * 1024);

        let single_layer_vram_bytes = self.moe_info.dense_backbone_bytes as usize / self.moe_info.moe_layer_count.max(1);

        self.dma_streamer = crate::cuda_dma_streamer::CudaDmaStreamer::initialize(
            self.n_gpu_layers as i32,
            single_layer_expert_chunk,
            single_layer_vram_bytes,
        );

        if self.dma_streamer.is_some() {
            eprintln!("🚀 [GgufMoeStreaming] Deferred DMA Streamer initialized with post-load VRAM headroom.");
        } else {
            eprintln!("⚠️ [GgufMoeStreaming] DMA Streamer not available (insufficient post-load VRAM).");
        }
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

        // 2. Stream Multi-Layer Active Experts & Lookahead Prefetch into GPU VRAM over PCIe DMA Highway
        if let (Some(streamer), Some(mmap)) = (&self.dma_streamer, &self.mmap) {
            let mmap_bytes: &[u8] = mmap.as_ref();
            let batch_capacity = streamer.layers_per_batch();
            
            // A. Dynamic Multi-Layer Batch Stream (current layer + ahead layers within batch capacity)
            let mut current_batch_slices: Vec<&[u8]> = Vec::new();
            for l_offset in 0..batch_capacity {
                let target_l = layer + l_offset;
                if target_l >= self.moe_info.moe_layer_count {
                    break;
                }
                for &expert_id in active_expert_ids {
                    if let Some(entry) = self.expert_index.lookup(target_l, expert_id) {
                        let gate_offset = entry.gate.file_offset as usize;
                        let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                        if gate_offset + total_len <= mmap_bytes.len() {
                            current_batch_slices.push(&mmap_bytes[gate_offset..gate_offset + total_len]);
                        }
                    }
                }
            }
            if !current_batch_slices.is_empty() {
                let total_batch_bytes: usize = current_batch_slices.iter().map(|s| s.len()).sum();
                let staged_layers = batch_capacity.min(self.moe_info.moe_layer_count.saturating_sub(layer));
                info!(
                    "⚡ [GgufMoeOffloading] Layer {} Batch Decision: Assembled {} expert slices ({:.2} MB) across {} layers bulk for PCIe DMA streaming",
                    layer,
                    current_batch_slices.len(),
                    total_batch_bytes as f64 / (1024.0 * 1024.0),
                    staged_layers
                );
                let _ = streamer.stream_batch_async(&current_batch_slices);
            }

            // B. Lookahead: Asynchronously prefetch Next Multi-Layer Batch into Ping-Pong Staging Slot
            let next_batch_start = layer + batch_capacity;
            if next_batch_start < self.moe_info.moe_layer_count {
                let mut prefetch_batch_slices: Vec<&[u8]> = Vec::new();
                for l_offset in 0..batch_capacity {
                    let target_l = next_batch_start + l_offset;
                    if target_l >= self.moe_info.moe_layer_count {
                        break;
                    }
                    for &next_expert_id in active_expert_ids {
                        if let Some(next_entry) = self.expert_index.lookup(target_l, next_expert_id) {
                            let gate_offset = next_entry.gate.file_offset as usize;
                            let total_len = (next_entry.gate.byte_length + next_entry.up.byte_length + next_entry.down.byte_length) as usize;
                            if gate_offset + total_len <= mmap_bytes.len() {
                                prefetch_batch_slices.push(&mmap_bytes[gate_offset..gate_offset + total_len]);
                            }
                        }
                    }
                }
                if !prefetch_batch_slices.is_empty() {
                    let _ = streamer.prefetch_batch_async(&prefetch_batch_slices);
                }
            }
        }

        // 3. Trigger Async Direct I/O Prefetch for Next Layer (Lookahead Overlap)
        if let Some(ref prefetcher) = self.async_prefetcher {
            if let Some(next_experts) = predicted_next_experts {
                let mut layer_map: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
                for &(nl, ne) in next_experts {
                    layer_map.entry(nl).or_default().push(ne);
                }
                for (nl, nes) in layer_map {
                    prefetcher.request_layer_prefetch(nl, &nes);
                }
            } else if layer + 1 < self.moe_info.moe_layer_count {
                // If router didn't pre-predict, prefetch the top hot experts of next layer
                prefetcher.request_layer_prefetch(layer + 1, active_expert_ids);
            }
        }

        // 4. Issue WILLNEED hints for currently active experts (ensure they stay in cache)
        for &expert_id in active_expert_ids {
            let key = (layer, expert_id);
            if let Some(pos) = self.last_active_experts.iter().position(|x| *x == key) {
                self.last_active_experts.remove(pos);
            }
            self.advise_willneed(layer, expert_id);
            self.last_active_experts.push(key);
        }

        // 5. Prefetch predicted next-step experts via OS advisory
        if let Some(next_experts) = predicted_next_experts {
            for &(next_layer, next_expert) in next_experts {
                self.advise_willneed(next_layer, next_expert);
            }
        }

        // 6. Dynamic LRU pool eviction bounded by Negotiator's RAM Budget
        let max_experts_in_ram = if self.moe_info.expert_size_bytes > 0 {
            let max_bytes = self.cache_budget_gb * 1024.0 * 1024.0 * 1024.0;
            (max_bytes / self.moe_info.expert_size_bytes as f64) as usize
        } else {
            16
        };

        // Evict oldest experts if we exceed our calculated RAM budget (ignoring pinned experts)
        while self.last_active_experts.len() > max_experts_in_ram {
            let (cold_layer, cold_expert) = self.last_active_experts.remove(0);
            if !self.pinned_hot_experts.contains(&(cold_layer, cold_expert)) {
                self.advise_dontneed(cold_layer, cold_expert);
            }
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
                    eprintln!("🧠 [GgufMoeStreaming] WILLNEED: Layer {}, Expert {} | Virtual Addr: {:p} | Size: {} bytes", layer, expert_id, ptr, total_len);
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
                    eprintln!("🧠 [GgufMoeStreaming] WILLNEED: Layer {}, Expert {} | Virtual Addr: {:p} | Size: {} bytes", layer, expert_id, ptr, total_len);
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
                    eprintln!("🧠 [GgufMoeStreaming] DONTNEED: Layer {}, Expert {} | Virtual Addr: {:p} | Size: {} bytes", layer, expert_id, ptr, total_len);
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_DONTNEED);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            // Note: On Windows file-backed mmaps (memmap2), VirtualUnlock is only valid on VirtualLock'd pages.
            // Calling VirtualUnlock on un-locked mmap pages causes ERROR_NOT_LOCKED and forces Windows
            // to invalidate active Standby Page Cache, leading to 100% NVMe SSD thrashing.
            // Windows Cache Manager automatically pages out un-accessed mmap pages when memory pressure occurs.
            if let (Some(mmap), Some(entry)) = (self.mmap.as_ref(), self.expert_index.lookup(layer, expert_id)) {
                let base = mmap.as_ptr();
                let mmap_len = mmap.len();
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= mmap_len {
                    // Soft advisory: avoid invalidating working set on Windows
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
        let temp_dir = std::env::temp_dir().join(format!("gguf_moe_controller_test_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let _ = std::fs::remove_dir_all(&temp_dir);
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

        // Clean up: drop controller first to release open mmap and file handles
        drop(controller);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

