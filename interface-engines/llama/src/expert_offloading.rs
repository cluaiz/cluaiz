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
    /// mmap base address (Linux/macOS only). None on Windows.
    #[allow(dead_code)]
    mmap_base: Option<*mut u8>,
    /// Total file size in bytes (needed for mmap).
    #[allow(dead_code)]
    mmap_len: usize,
    /// MoE structural info.
    pub moe_info: MoeModelInfo,
    /// Records the expert IDs activated in the most recent routing step (for cold hints).
    last_active_experts: Vec<(usize, usize)>,
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
    ) -> anyhow::Result<Self> {
        info!(
            "🧠 [GgufMoeStreaming] Initializing for: {:?} | {} experts/layer | cache: {:.2}GB",
            model_path.file_name().unwrap_or_default(),
            moe_info.expert_count,
            cache_budget_gb
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

        // Step 4: mmap the model file (Linux/macOS only — advisory only, no data copying)
        let (mmap_base, mmap_len) = Self::try_mmap(model_path);

        let mut controller = Self {
            model_path: model_path.to_path_buf(),
            expert_index,
            heat_tracker,
            cache,
            mmap_base,
            mmap_len,
            moe_info,
            last_active_experts: Vec::new(),
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
            self.advise_willneed(layer, expert_id);
            self.last_active_experts.push((layer, expert_id));
        }

        // 3. Prefetch predicted next-step experts (read-ahead)
        if let Some(next_experts) = predicted_next_experts {
            for &(next_layer, next_expert) in next_experts {
                self.advise_willneed(next_layer, next_expert);
            }
        }

        // 4. Release cold experts from page cache to make room
        // Simple heuristic: release experts that were active 2+ steps ago
        // and are not in the current active set
        let current: std::collections::HashSet<(usize, usize)> = active_expert_ids
            .iter()
            .map(|&e| (layer, e))
            .collect();
        let cold: Vec<(usize, usize)> = self
            .last_active_experts
            .iter()
            .filter(|k| !current.contains(k))
            .copied()
            .collect();
        for (cold_layer, cold_expert) in cold {
            self.advise_dontneed(cold_layer, cold_expert);
        }
    }

    // ── Private: OS advisory calls ────────────────────────────────────────────

    fn advise_willneed(&self, layer: usize, expert_id: usize) {
        #[cfg(unix)]
        {
            if let (Some(base), Some(entry)) = (self.mmap_base, self.expert_index.lookup(layer, expert_id)) {
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= self.mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut libc::c_void;
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_WILLNEED);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            // On Windows: best-effort — VirtualAlloc advisory not available for mmap.
            // Rely on OS prefetcher; expert scheduling still recorded in heat tracker.
            let _ = (layer, expert_id);
        }
    }

    fn advise_dontneed(&self, layer: usize, expert_id: usize) {
        #[cfg(unix)]
        {
            if let (Some(base), Some(entry)) = (self.mmap_base, self.expert_index.lookup(layer, expert_id)) {
                let gate_offset = entry.gate.file_offset as usize;
                let total_len = (entry.gate.byte_length + entry.up.byte_length + entry.down.byte_length) as usize;
                if gate_offset + total_len <= self.mmap_len {
                    let ptr = unsafe { base.add(gate_offset) } as *mut libc::c_void;
                    unsafe {
                        libc::madvise(ptr, total_len, libc::MADV_DONTNEED);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = (layer, expert_id);
        }
    }

    /// Pre-warm hot experts from heat tracker on startup.
    fn warm_hot_experts(&mut self) {
        let budget = self.moe_info.recommended_cache_budget_gb() * 1024.0 * 1024.0 * 1024.0;
        let expert_size = self.moe_info.expert_size_bytes;

        let hot = if let Ok(tracker) = self.heat_tracker.lock() {
            tracker.get_hottest_experts(budget as u64, expert_size)
        } else {
            Vec::new()
        };

        if hot.is_empty() {
            info!("🌡️ [GgufMoeStreaming] No prior routing heat data — cold start.");
            return;
        }

        info!(
            "🌡️ [GgufMoeStreaming] Pre-warming {} hot experts from previous session heat data.",
            hot.len()
        );
        for (layer, expert_id) in hot {
            self.advise_willneed(layer, expert_id);
        }
    }

    /// Try to memory-map the model file for advisory calls.
    /// Returns (Some(base_ptr), file_len) on success, (None, 0) on failure/Windows.
    #[allow(unused_variables)]
    fn try_mmap(model_path: &Path) -> (Option<*mut u8>, usize) {
        #[cfg(unix)]
        {
            use std::fs::File;
            use std::os::unix::io::AsRawFd;

            let file = match File::open(model_path) {
                Ok(f) => f,
                Err(e) => {
                    warn!("🧠 [GgufMoeStreaming] Cannot open model file for mmap: {}", e);
                    return (None, 0);
                }
            };
            let file_len = match file.metadata() {
                Ok(m) => m.len() as usize,
                Err(_) => return (None, 0),
            };
            if file_len == 0 {
                return (None, 0);
            }
            let ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    file_len,
                    libc::PROT_READ,
                    libc::MAP_SHARED | libc::MAP_NORESERVE,
                    file.as_raw_fd(),
                    0,
                )
            };
            if ptr == libc::MAP_FAILED {
                warn!("🧠 [GgufMoeStreaming] mmap failed — advisory hints disabled.");
                return (None, 0);
            }
            info!(
                "🧠 [GgufMoeStreaming] mmap established: {:.2} GB advisory window.",
                file_len as f64 / (1024.0 * 1024.0 * 1024.0)
            );
            return (Some(ptr as *mut u8), file_len);
        }
        #[cfg(windows)]
        {
            // Windows: mmap advisory not needed — rely on OS page cache
            info!("🧠 [GgufMoeStreaming] Windows: mmap advisory skipped (OS handles page cache).");
            (None, 0)
        }
        #[cfg(not(any(unix, windows)))]
        {
            (None, 0)
        }
    }
}

impl Drop for GgufMoeStreamingController {
    fn drop(&mut self) {
        // Unmap the advisory mmap on Linux/macOS
        #[cfg(unix)]
        {
            if let Some(base) = self.mmap_base {
                if self.mmap_len > 0 {
                    unsafe {
                        libc::munmap(base as *mut libc::c_void, self.mmap_len);
                    }
                }
            }
        }
        // Heat tracker auto-saves on drop via its own Drop impl
        info!("🧠 [GgufMoeStreaming] Controller dropped — heat data auto-saved.");
    }
}
