//! 🧠 MoE Expert LRU Cache Manager
//! Colibri Architecture: Manages an in-RAM LRU cache pool for active MoE expert weights.
//! Prevents Out-Of-Memory (OOM) crashes by locking active experts in RAM and evicting
//! least-recently-used experts back to disk page storage when memory limits are reached.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

/// A single loaded expert tensor buffer in RAM.
#[derive(Debug, Clone)]
pub struct LoadedExpertBlock {
    pub expert_id: usize,
    pub layer_index: usize,
    pub size_bytes: usize,
    pub weights_data: Arc<Vec<u8>>,
}

/// Key identifying a specific expert in a specific layer.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExpertKey {
    pub layer_index: usize,
    pub expert_id: usize,
}

/// The LRU Cache Manager for MoE Expert blocks.
pub struct ExpertCacheManager {
    /// Maximum RAM capacity in bytes allowed for cached experts.
    max_ram_bytes: usize,
    /// Current RAM consumed by cached experts in bytes.
    current_ram_bytes: usize,
    /// Fast map lookup for cached experts.
    cache_map: HashMap<ExpertKey, LoadedExpertBlock>,
    /// LRU access queue: Most recently used at back, least recently used at front.
    lru_queue: VecDeque<ExpertKey>,
}

impl ExpertCacheManager {
    /// Creates a new ExpertCacheManager with a given RAM capacity budget (in GB).
    pub fn new(max_ram_gb: f64) -> Self {
        let max_ram_bytes = (max_ram_gb * 1024.0 * 1024.0 * 1024.0) as usize;
        info!(
            "💾 [MoE-Cache] Initialized ExpertCacheManager | Capacity: {:.2} GB ({} bytes)",
            max_ram_gb, max_ram_bytes
        );
        Self {
            max_ram_bytes,
            current_ram_bytes: 0,
            cache_map: HashMap::new(),
            lru_queue: VecDeque::new(),
        }
    }

    /// Fetches an expert block from the cache if present.
    /// Updates LRU access ordering.
    pub fn get(&mut self, layer_index: usize, expert_id: usize) -> Option<LoadedExpertBlock> {
        let key = ExpertKey { layer_index, expert_id };
        if let Some(block) = self.cache_map.get(&key) {
            // Move key to back of LRU queue (mark as most recently used)
            if let Some(pos) = self.lru_queue.iter().position(|k| k == &key) {
                self.lru_queue.remove(pos);
            }
            self.lru_queue.push_back(key);
            Some(block.clone())
        } else {
            None
        }
    }

    /// Inserts a newly streamed expert block into the RAM cache.
    /// Performs LRU eviction if insertion would exceed max_ram_bytes.
    pub fn insert(&mut self, block: LoadedExpertBlock) {
        let key = ExpertKey {
            layer_index: block.layer_index,
            expert_id: block.expert_id,
        };

        // If key already exists, replace it
        if let Some(old) = self.cache_map.remove(&key) {
            self.current_ram_bytes = self.current_ram_bytes.saturating_sub(old.size_bytes);
            if let Some(pos) = self.lru_queue.iter().position(|k| k == &key) {
                self.lru_queue.remove(pos);
            }
        }

        // Evict LRU entries until new block fits in RAM
        while (self.current_ram_bytes + block.size_bytes) > self.max_ram_bytes && !self.lru_queue.is_empty() {
            if let Some(evict_key) = self.lru_queue.pop_front() {
                if let Some(evicted) = self.cache_map.remove(&evict_key) {
                    self.current_ram_bytes = self.current_ram_bytes.saturating_sub(evicted.size_bytes);
                    warn!(
                        "🔄 [MoE-Cache] LRU Evicted Expert L{}E{} ({:.2} MB freed)",
                        evicted.layer_index,
                        evicted.expert_id,
                        (evicted.size_bytes as f64) / (1024.0 * 1024.0)
                    );
                }
            }
        }

        self.current_ram_bytes += block.size_bytes;
        self.cache_map.insert(key.clone(), block);
        self.lru_queue.push_back(key);
    }

    /// Returns the current RAM usage in megabytes.
    pub fn current_usage_mb(&self) -> f64 {
        (self.current_ram_bytes as f64) / (1024.0 * 1024.0)
    }

    /// Returns the number of experts currently cached in RAM.
    pub fn len(&self) -> usize {
        self.cache_map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache_map.is_empty()
    }

    /// Clears all cached experts from RAM.
    pub fn clear(&mut self) {
        self.cache_map.clear();
        self.lru_queue.clear();
        self.current_ram_bytes = 0;
        info!("🧹 [MoE-Cache] Expert Cache pool cleared.");
    }
}

/// Thread-safe wrapper for ExpertCacheManager.
#[derive(Clone)]
pub struct SharedExpertCache(pub Arc<Mutex<ExpertCacheManager>>);

impl SharedExpertCache {
    pub fn new(max_ram_gb: f64) -> Self {
        Self(Arc::new(Mutex::new(ExpertCacheManager::new(max_ram_gb))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expert_cache_lru_eviction() {
        let gb_multiplier = 1024.0 * 1024.0 * 1024.0;
        let mut manager = ExpertCacheManager::new(300.0 / gb_multiplier);

        assert_eq!(manager.max_ram_bytes, 300);
        assert_eq!(manager.current_ram_bytes, 0);

        let block1 = LoadedExpertBlock {
            expert_id: 1,
            layer_index: 0,
            size_bytes: 100,
            weights_data: Arc::new(vec![0u8; 100]),
        };
        let block2 = LoadedExpertBlock {
            expert_id: 2,
            layer_index: 0,
            size_bytes: 100,
            weights_data: Arc::new(vec![0u8; 100]),
        };
        let block3 = LoadedExpertBlock {
            expert_id: 3,
            layer_index: 0,
            size_bytes: 100,
            weights_data: Arc::new(vec![0u8; 100]),
        };

        manager.insert(block1.clone());
        manager.insert(block2.clone());
        manager.insert(block3.clone());

        assert_eq!(manager.len(), 3);
        assert_eq!(manager.current_ram_bytes, 300);

        assert!(manager.get(0, 1).is_some());

        let block4 = LoadedExpertBlock {
            expert_id: 4,
            layer_index: 0,
            size_bytes: 100,
            weights_data: Arc::new(vec![0u8; 100]),
        };
        manager.insert(block4);

        assert_eq!(manager.len(), 3);
        assert_eq!(manager.current_ram_bytes, 300);

        assert!(manager.get(0, 2).is_none());
        assert!(manager.get(0, 1).is_some());
        assert!(manager.get(0, 3).is_some());
        assert!(manager.get(0, 4).is_some());
    }
}

