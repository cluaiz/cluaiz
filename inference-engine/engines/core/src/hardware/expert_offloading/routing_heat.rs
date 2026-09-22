//! 🌡️ Routing Heat Tracker
//! Tracks which MoE experts are "hot" (frequently activated) across inference sessions.
//! Persists routing statistics to `.cluaiz_routing_heat` in the model directory.
//!
//! Evidence basis (Colibri's route_trace.h):
//! Colibri maintains a `.coli_usage` file: "layer expert count\n" (sparse text format).
//! Their engine "gets faster the more you use it" because hot experts get pinned to RAM.
//!
//! Our format mirrors Colibri's: one record per line → "{layer} {expert} {count}"
//! Two header lines:
//!   "-1 {n_layers} {n_experts}"   → dimensions
//!   "-2 {format_version} 0"        → version marker
//!
//! Hot experts (above a frequency threshold) are recommended for pinning in the LRU cache
//! so they are never evicted, reducing SSD reads for common query patterns.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

const FORMAT_VERSION: u32 = 1;
const HEAT_FILE_NAME: &str = ".cluaiz_routing_heat";

/// Tracks routing heat for a specific model.
pub struct RoutingHeatTracker {
    /// counts[layer][expert] = total activation count across all sessions.
    counts: Vec<Vec<u32>>,
    pub n_layers: usize,
    pub n_experts: usize,
    /// Path to the persistent heat file in the model directory.
    heat_file_path: PathBuf,
    /// Total number of routing decisions recorded in this session.
    session_records: u64,
}

impl RoutingHeatTracker {
    /// Create a new tracker for a model. Loads existing heat data if `.cluaiz_routing_heat` exists.
    pub fn new(n_layers: usize, n_experts: usize, model_dir: &Path) -> Self {
        let heat_file_path = model_dir.join(HEAT_FILE_NAME);
        let mut tracker = Self {
            counts: vec![vec![0u32; n_experts]; n_layers],
            n_layers,
            n_experts,
            heat_file_path: heat_file_path.clone(),
            session_records: 0,
        };

        // Load existing heat data from previous sessions
        if heat_file_path.exists() {
            match tracker.load_from_disk() {
                Ok(loaded) => info!(
                    "🌡️ [HeatTracker] Loaded {} routing records from previous sessions.",
                    loaded
                ),
                Err(e) => warn!("🌡️ [HeatTracker] Could not load heat file: {}", e),
            }
        } else {
            info!("🌡️ [HeatTracker] No previous heat data found. Starting fresh.");
        }

        tracker
    }

    /// Record that specific experts were activated in a given layer for this token.
    /// Call this after each MoE routing decision.
    pub fn record_routing(&mut self, layer: usize, expert_ids: &[usize]) {
        if layer >= self.n_layers {
            return;
        }
        for &expert_id in expert_ids {
            if expert_id < self.n_experts {
                self.counts[layer][expert_id] = self.counts[layer][expert_id].saturating_add(1);
                self.session_records += 1;
            }
        }
    }

    /// Returns the top-N hottest experts sorted by activation count.
    /// `budget_bytes`: maximum RAM bytes available for pinning.
    /// `expert_size_bytes`: estimated size of one expert in bytes.
    ///
    /// Returns a list of (layer, expert_id) pairs to pin in the LRU cache.
    pub fn get_hottest_experts(&self, budget_bytes: u64, expert_size_bytes: u64) -> Vec<(usize, usize)> {
        if expert_size_bytes == 0 || budget_bytes == 0 {
            return Vec::new();
        }

        let max_pinned = (budget_bytes / expert_size_bytes.max(1)) as usize;
        if max_pinned == 0 {
            return Vec::new();
        }

        // Collect all (layer, expert_id, count) triples
        let mut all_experts: Vec<(usize, usize, u32)> = Vec::new();
        for (layer, expert_row) in self.counts.iter().enumerate() {
            for (expert_id, &count) in expert_row.iter().enumerate() {
                if count > 0 {
                    all_experts.push((layer, expert_id, count));
                }
            }
        }

        // Sort descending by count (hottest first)
        all_experts.sort_unstable_by(|a, b| b.2.cmp(&a.2));

        let pinned: Vec<(usize, usize)> = all_experts
            .into_iter()
            .take(max_pinned)
            .map(|(l, e, _)| (l, e))
            .collect();

        info!(
            "🌡️ [HeatTracker] Recommending {} hot experts to pin (budget: {:.2} MB, expert_size: {:.2} MB)",
            pinned.len(),
            budget_bytes as f64 / (1024.0 * 1024.0),
            expert_size_bytes as f64 / (1024.0 * 1024.0),
        );

        pinned
    }

    /// Returns the activation frequency of a specific expert in a layer.
    pub fn get_expert_frequency(&self, layer: usize, expert_id: usize) -> u32 {
        if layer < self.n_layers && expert_id < self.n_experts {
            self.counts[layer][expert_id]
        } else {
            0
        }
    }

    /// Persist routing heat to `.cluaiz_routing_heat` in the model directory.
    /// Mirrors Colibri's `.coli_usage` file format for future cross-tool compatibility.
    pub fn save(&self) -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&self.heat_file_path)?;

        // Header line 1: dimensions
        writeln!(file, "-1 {} {}", self.n_layers, self.n_experts)?;
        // Header line 2: format version
        writeln!(file, "-2 {} 0", FORMAT_VERSION)?;

        // Data: sparse — only write non-zero counts
        let mut written = 0u64;
        for (layer, expert_row) in self.counts.iter().enumerate() {
            for (expert_id, &count) in expert_row.iter().enumerate() {
                if count > 0 {
                    writeln!(file, "{} {} {}", layer, expert_id, count)?;
                    written += 1;
                }
            }
        }

        info!(
            "🌡️ [HeatTracker] Saved {} routing records to {:?} (session: {} new records)",
            written,
            self.heat_file_path.file_name().unwrap_or_default(),
            self.session_records,
        );
        Ok(())
    }

    /// Load routing heat from disk. Returns number of records loaded.
    fn load_from_disk(&mut self) -> anyhow::Result<u64> {
        let file = std::fs::File::open(&self.heat_file_path)?;
        let reader = BufReader::new(file);

        let mut loaded = 0u64;
        let mut file_n_layers = self.n_layers;
        let mut file_n_experts = self.n_experts;

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }

            let col0 = parts[0].parse::<i64>().unwrap_or(0);
            let col1 = parts[1].parse::<u64>().unwrap_or(0);
            let col2 = parts[2].parse::<u64>().unwrap_or(0);

            // Parse header lines (negative layer field = metadata)
            if col0 == -1 {
                // Dimensions header: "-1 n_layers n_experts"
                file_n_layers = col1 as usize;
                file_n_experts = col2 as usize;
                continue;
            }
            if col0 == -2 {
                // Version header: skip
                continue;
            }

            // Data record: "layer expert count"
            let layer = col0 as usize;
            let expert = col1 as usize;
            let count = col2 as u32;

            if layer < self.n_layers && expert < self.n_experts {
                self.counts[layer][expert] = self.counts[layer][expert].saturating_add(count);
                loaded += 1;
            }
        }

        // Warn on dimension mismatch (model may have changed quantization)
        if file_n_layers != self.n_layers || file_n_experts != self.n_experts {
            warn!(
                "🌡️ [HeatTracker] Dimension mismatch: file={}×{}, model={}×{}. Partial data loaded.",
                file_n_layers, file_n_experts, self.n_layers, self.n_experts
            );
        }

        Ok(loaded)
    }

    /// Returns total routing decisions tracked across all sessions.
    pub fn total_records(&self) -> u64 {
        self.counts
            .iter()
            .flat_map(|row| row.iter())
            .map(|&c| c as u64)
            .sum()
    }

    /// Returns the activation count for a specific expert.
    pub fn get_count(&self, layer: usize, expert_id: usize) -> u32 {
        if layer < self.n_layers && expert_id < self.n_experts {
            self.counts[layer][expert_id]
        } else {
            0
        }
    }
}

/// Auto-saves heat data when the tracker goes out of scope.
impl Drop for RoutingHeatTracker {
    fn drop(&mut self) {
        if self.session_records > 0 {
            if let Err(e) = self.save() {
                warn!("🌡️ [HeatTracker] Auto-save failed on drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_heat_tracker() {
        let temp_dir = std::env::temp_dir();
        let tracker_path = temp_dir.join(".cluaiz_routing_heat");
        if tracker_path.exists() {
            let _ = std::fs::remove_file(&tracker_path);
        }

        // 1. Create a fresh tracker with 4 layers and 8 experts
        let mut tracker = RoutingHeatTracker::new(4, 8, &temp_dir);
        assert_eq!(tracker.n_layers, 4);
        assert_eq!(tracker.n_experts, 8);
        assert_eq!(tracker.total_records(), 0);

        // 2. Record routing decisions
        tracker.record_routing(0, &[2, 5]);
        tracker.record_routing(0, &[2]);
        tracker.record_routing(1, &[7]);

        assert_eq!(tracker.get_count(0, 2), 2);
        assert_eq!(tracker.get_count(0, 5), 1);
        assert_eq!(tracker.get_count(1, 7), 1);
        assert_eq!(tracker.get_count(2, 0), 0);
        assert_eq!(tracker.total_records(), 4);

        // 3. Recommended hot experts
        let hot = tracker.get_hottest_experts(250, 100);
        assert_eq!(hot.len(), 2);
        assert_eq!(hot[0], (0, 2));
        assert!(hot[1] == (0, 5) || hot[1] == (1, 7));

        // 4. Save to disk
        tracker.save().unwrap();
        assert!(tracker_path.exists());

        // 5. Load in a new tracker
        let new_tracker = RoutingHeatTracker::new(4, 8, &temp_dir);
        assert_eq!(new_tracker.get_count(0, 2), 2);
        assert_eq!(new_tracker.get_count(0, 5), 1);
        assert_eq!(new_tracker.get_count(1, 7), 1);
        assert_eq!(new_tracker.total_records(), 4);

        // Cleanup
        let _ = std::fs::remove_file(&tracker_path);
    }
}

