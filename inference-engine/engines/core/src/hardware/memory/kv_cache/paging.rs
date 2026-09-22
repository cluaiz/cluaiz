use crate::hardware::memory::kv_cache::block::PhysicalBlock;
use std::collections::HashMap;

/// 🧠 BlockManager: The Silicon Orchestrator for paged VRAM.
/// It maintains a global free-list and handles logical-to-physical mapping.
pub struct BlockManager {
    pub total_blocks: usize,
    pub block_size: usize, // Standard: 16 tokens
    pub blocks: Vec<PhysicalBlock>,
    /// Map of sequence_id to list of physical_block_indices
    pub page_table: HashMap<String, Vec<usize>>,
}

impl BlockManager {
    pub fn new(total_vram_mb: usize, hidden_size: usize, block_size: usize) -> Self {
        // Rough calculation: Block size in bytes = 16 * hidden_size * 2 (K+V) * dtype_size
        // 16 * 4096 * 2 * 2 (fp16) = 256KB per block.
        let bytes_per_block = block_size * hidden_size * 4; 
        let total_blocks = (total_vram_mb * 1024 * 1024) / bytes_per_block;
        
        tracing::info!("🧱 [Memory] Initializing Paged Pool: {} blocks (Size: {} tokens each)", total_blocks, block_size);

        let mut blocks = Vec::with_capacity(total_blocks);
        for i in 0..total_blocks {
            blocks.push(PhysicalBlock::new(i, block_size));
        }

        Self {
            total_blocks,
            block_size,
            blocks,
            page_table: HashMap::new(),
        }
    }

    /// Allocates the next free block for a specific sequence.
    pub fn allocate_block(&mut self, sequence_id: &str) -> Option<usize> {
        let block_idx = self.blocks.iter().position(|b| b.is_free)?;
        self.blocks[block_idx].mark_used();
        
        self.page_table.entry(sequence_id.to_string())
            .or_default()
            .push(block_idx);
            
        Some(block_idx)
    }

    /// Releases all blocks associated with a sequence.
    pub fn release_sequence(&mut self, sequence_id: &str) {
        if let Some(indices) = self.page_table.remove(sequence_id) {
            for idx in indices {
                if idx < self.blocks.len() {
                    self.blocks[idx].release();
                }
            }
        }
    }

    pub fn get_vram_usage_percent(&self) -> f32 {
        let used = self.blocks.iter().filter(|b| !b.is_free).count();
        (used as f32 / self.total_blocks as f32) * 100.0
    }
}
