//! 🧠 ONNX MoE Expert Session Pool
//! Manages a dynamic InferenceSession pool for ONNX MoE models.
//!
//! ## Core Constraint
//! ONNX Runtime loads the entire `.onnx` graph into memory in a single `InferenceSession`.
//! For MoE models with external data (`.onnx.data` shards), each expert's tensor
//! data is stored in a separate `.bin` or `.onnx.data` external data file.
//!
//! ONNX Runtime 1.17+ supports `SessionOptions::add_external_initializers()` which allows
//! loading only specific named initializers (expert weight tensors) on demand.
//!
//! ## Strategy
//! - At model initialization: Load a SHARED "backbone" session (attention, shared FFN, embeddings)
//!   that excludes all routed expert weight tensors (set via `disable_external_initializers`).
//! - Maintain a small pool of "expert sessions" — each loads a subset of expert weights.
//! - On each token: activate sessions covering the top-K required experts.
//! - Apply LRU eviction to keep the pool size within the expert cache RAM budget.
//!
//! ## Current Implementation Status
//! ONNX Runtime's Rust bindings (`ort` crate) don't yet expose the full
//! external initializer API needed for true per-expert loading. This module
//! implements the detection logic, session pool structure, and the integration
//! hooks. The actual expert-selective loading is a NO-OP on Windows (falls back
//! to full model load) until the ONNX external initializer API is stable in `ort`.
//!
//! Evidence: ONNX Runtime GitHub issue #14158 (External Initializer Selective Load).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use cluaiz_shared::hardware::expert_offloading::{MoeModelInfo, RoutingHeatTracker, SharedExpertCache};

// ─── Expert Shard Manifest ────────────────────────────────────────────────────

/// Maps expert tensor names to their external data file and byte range.
/// Built by scanning the ONNX external data manifest JSON.
#[derive(Debug, Clone)]
pub struct ExpertShardEntry {
    /// Tensor name in the ONNX graph (e.g. "model.layers.4.mlp.experts.2.gate_proj.weight")
    pub tensor_name: String,
    /// Layer index this expert belongs to.
    pub layer: usize,
    /// Expert ID within this layer.
    pub expert_id: usize,
    /// Path to the external data file containing this tensor.
    pub data_file: PathBuf,
    /// Byte offset within the data file.
    pub offset: u64,
    /// Byte length of this tensor.
    pub length: u64,
}

/// Complete expert shard manifest for an ONNX MoE model.
pub struct OnnxExpertShardManifest {
    /// All expert shard entries, indexed by (layer, expert_id).
    pub entries: HashMap<(usize, usize), Vec<ExpertShardEntry>>,
    pub n_experts: usize,
    pub n_layers: usize,
}

impl OnnxExpertShardManifest {
    /// Build the manifest by scanning the ONNX external data files.
    /// Reads `{model_stem}.onnx.data` or `model.onnx_data` manifest if present.
    pub fn from_model_dir(model_path: &Path, moe_info: &MoeModelInfo) -> Self {
        let model_dir = model_path.parent().unwrap_or(Path::new("."));

        // Scan for external data files in the model directory
        let data_files: Vec<PathBuf> = std::fs::read_dir(model_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|ext| ext == "data" || ext == "bin")
                            .unwrap_or(false)
                            || p.to_string_lossy().contains(".onnx.data")
                    })
                    .collect()
            })
            .unwrap_or_default();

        if data_files.is_empty() {
            warn!(
                "🧠 [OnnxMoePool] No external data files found in {:?}. \
                 Expert offloading requires .onnx.data shards.",
                model_dir
            );
        } else {
            info!(
                "🧠 [OnnxMoePool] Found {} external data file(s) for expert offloading.",
                data_files.len()
            );
        }

        // NOTE: Full tensor-name → file mapping requires parsing the ONNX protobuf header.
        // The `ort` crate does not expose this directly. We create a placeholder manifest
        // that marks data files as available — the session pool uses them via ONNX RT's
        // built-in external data loading when constructing selective sessions.
        OnnxExpertShardManifest {
            entries: HashMap::new(), // Populated when ONNX RT external initializer API is available
            n_experts: moe_info.expert_count,
            n_layers: moe_info.moe_layer_count,
        }
    }
}

// ─── Session Pool ─────────────────────────────────────────────────────────────

/// A pooled ONNX InferenceSession slot for a subset of expert weights.
/// In future: each slot will hold a selective session for a specific expert group.
/// For now: represents a session handle placeholder.
pub struct ExpertSessionSlot {
    pub layer: usize,
    pub expert_ids: Vec<usize>,
    pub last_used: std::time::Instant,
    /// ONNX session handle — currently None until selective loading API is stable.
    pub session_handle: Option<Box<dyn std::any::Any + Send>>,
}

/// Dynamic ONNX InferenceSession pool for MoE expert on-demand loading.
pub struct OnnxMoeSessionPool {
    /// Detected MoE structural metadata.
    pub moe_info: MoeModelInfo,
    /// Expert shard manifest (tensor → external data file mapping).
    pub manifest: Arc<OnnxExpertShardManifest>,
    /// Active session slots (limited to cache budget).
    slots: Vec<ExpertSessionSlot>,
    /// Maximum number of concurrent expert sessions in pool.
    max_slots: usize,
    /// Routing heat tracker for cross-session optimization.
    heat_tracker: Arc<Mutex<RoutingHeatTracker>>,
    /// LRU expert cache (shared with the rest of the expert offloading subsystem).
    cache: SharedExpertCache,
}

impl OnnxMoeSessionPool {
    /// Initialize the session pool for an ONNX MoE model.
    pub fn new(model_path: &Path, moe_info: MoeModelInfo, cache_budget_gb: f64) -> anyhow::Result<Self> {
        info!(
            "🧠 [OnnxMoePool] Initializing session pool: {} experts/layer | cache: {:.2}GB",
            moe_info.expert_count, cache_budget_gb
        );

        let manifest = Arc::new(OnnxExpertShardManifest::from_model_dir(model_path, &moe_info));

        let model_dir = model_path.parent().unwrap_or(Path::new("."));
        let heat_tracker = RoutingHeatTracker::new(
            moe_info.moe_layer_count,
            moe_info.expert_count,
            model_dir,
        );
        let heat_tracker = Arc::new(Mutex::new(heat_tracker));

        let cache = SharedExpertCache::new(cache_budget_gb);

        // Estimate max concurrent expert sessions: cache_budget / estimated expert session RAM overhead
        // Expert session overhead ≈ expert_size_bytes (weights) + ~50MB ONNX RT overhead per session
        let session_overhead_bytes = moe_info.expert_size_bytes
            + (50 * 1024 * 1024); // 50MB ONNX RT overhead estimate
        let max_slots = if session_overhead_bytes > 0 {
            ((cache_budget_gb * 1024.0 * 1024.0 * 1024.0) as u64
                / session_overhead_bytes.max(1))
            .max(2) as usize // Minimum 2 sessions (current + prefetch)
        } else {
            4 // Default fallback
        };

        info!(
            "🧠 [OnnxMoePool] Max concurrent expert sessions: {} (cache {:.2}GB / ~{:.2}MB per session)",
            max_slots,
            cache_budget_gb,
            session_overhead_bytes as f64 / (1024.0 * 1024.0)
        );

        Ok(Self {
            moe_info,
            manifest,
            slots: Vec::with_capacity(max_slots),
            max_slots,
            heat_tracker,
            cache,
        })
    }

    /// Called before each token generation step with the predicted active experts.
    /// Ensures the required expert sessions are loaded and warm.
    ///
    /// Returns Ok(()) if sessions are ready, Err if loading fails.
    pub fn prepare_experts(&mut self, layer: usize, expert_ids: &[usize]) -> anyhow::Result<()> {
        // Record routing heat
        if let Ok(mut tracker) = self.heat_tracker.lock() {
            tracker.record_routing(layer, expert_ids);
        }

        // Check if these experts are already in the pool
        let already_loaded = self.slots.iter().any(|s| {
            s.layer == layer && expert_ids.iter().all(|e| s.expert_ids.contains(e))
        });

        if already_loaded {
            // Update last_used timestamp for LRU
            if let Some(slot) = self.slots.iter_mut().find(|s| {
                s.layer == layer && expert_ids.iter().all(|e| s.expert_ids.contains(e))
            }) {
                slot.last_used = std::time::Instant::now();
            }
            return Ok(());
        }

        // Evict LRU slot if pool is full
        if self.slots.len() >= self.max_slots {
            self.evict_lru();
        }

        // Create a new slot placeholder
        // NOTE: Actual ONNX selective session loading will be implemented here
        // once `ort` crate exposes `SessionOptions::with_external_initializers()`.
        // For now, this records the intent — the backbone session handles all experts.
        self.slots.push(ExpertSessionSlot {
            layer,
            expert_ids: expert_ids.to_vec(),
            last_used: std::time::Instant::now(),
            session_handle: None, // Placeholder — populated by ONNX selective load
        });

        Ok(())
    }

    /// Evict the least recently used expert session slot.
    fn evict_lru(&mut self) {
        if let Some(idx) = self
            .slots
            .iter()
            .enumerate()
            .min_by_key(|(_, s)| s.last_used)
            .map(|(i, _)| i)
        {
            let evicted = self.slots.remove(idx);
            warn!(
                "🔄 [OnnxMoePool] Evicted expert session: layer={} experts={:?}",
                evicted.layer, evicted.expert_ids
            );
        }
    }

    /// Returns current number of active expert sessions.
    pub fn active_sessions(&self) -> usize {
        self.slots.len()
    }
}
