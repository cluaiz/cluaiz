use serde::{Deserialize, Serialize};

/// 🧱 PhysicalBlock: The atomic unit of Sovereign Memory.
/// Each block holds a fixed set of 'neural buckets' (tokens) for zero-fragmentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhysicalBlock {
    pub id: usize,
    pub token_capacity: usize, // Standard: 16
    pub is_free: bool,
    pub last_access: u64, // For LRU eviction logic
}

impl PhysicalBlock {
    pub fn new(id: usize, capacity: usize) -> Self {
        Self {
            id,
            token_capacity: capacity,
            is_free: true,
            last_access: 0,
        }
    }

    pub fn mark_used(&mut self) {
        self.is_free = false;
        self.last_access = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    pub fn release(&mut self) {
        self.is_free = true;
    }
}
