use crate::hardware::memory::kv_cache::paging::BlockManager;
use anyhow::Result;

pub mod block;
pub mod paging;
pub mod stitching;
pub mod eviction;

/// 🧬 PagedKVCache: The Logical Sovereign Memory Controller.
/// Manages block assignments without holding raw framework tensors.
pub struct PagedKVCache {
    pub sequence_id: String,
    pub block_manager: std::sync::Arc<std::sync::Mutex<BlockManager>>,
    pub assigned_blocks: Vec<usize>,
    pub current_block_usage: usize,
    pub head_count_kv: usize,
    pub head_dim: usize,
    pub max_blocks: usize,
}

impl PagedKVCache {
    pub fn new(
        sequence_id: &str,
        head_count_kv: usize,
        head_dim: usize,
        max_context: usize,
        block_manager: std::sync::Arc<std::sync::Mutex<BlockManager>>
    ) -> Self {
        let block_size = 16; // Fixed Sovereign Block Size
        let max_blocks = max_context / block_size;

        Self {
            sequence_id: sequence_id.to_string(),
            block_manager,
            assigned_blocks: Vec::new(),
            current_block_usage: 0,
            head_count_kv,
            head_dim,
            max_blocks,
        }
    }

    /// 🧱 Core Append: Manages block allocation logic.
    pub fn append_slot(&mut self, tokens_to_add: usize) -> Result<usize> {
        let block_size = 16;

        if self.assigned_blocks.is_empty() || self.current_block_usage + tokens_to_add > block_size {
            // Requesting new silicon slot from manager
            let mut mg = self.block_manager.lock().unwrap();
            if let Some(idx) = mg.allocate_block(&self.sequence_id) {
                self.assigned_blocks.push(idx);
                self.current_block_usage = tokens_to_add;
                Ok(idx)
            } else {
                anyhow::bail!("SILICON_VRAM_EXHAUSTED")
            }
        } else {
            self.current_block_usage += tokens_to_add;
            Ok(*self.assigned_blocks.last().unwrap())
        }
    }

    /// 🔗 LogitSteer Injection: Mapping history blocks.
    pub fn inject_block(&mut self, block_id: usize) -> Result<()> {
        self.assigned_blocks.insert(0, block_id);
        Ok(())
    }

    pub fn get_block_map(&self) -> &Vec<usize> {
        &self.assigned_blocks
    }
}
