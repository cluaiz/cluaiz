//! 🔄 Fixed Static Double-Buffer (Ring Buffer) for Zero-Copy MoE Expert Staging
//! Eliminates pointer reallocation and CUDA address desynchronization (`0xc0000005`).
//!
//! Provides two pre-allocated 4KB-aligned 64MB staging slots (Slot A and Slot B):
//! - Slot A is actively read by CPU / GPU kernel during Layer $N$ computation.
//! - Slot B is concurrently filled by Direct I/O background worker for Layer $N+1$.
//! - When Layer $N$ completes, slots ping-pong swap instantly with zero reallocation overhead.

use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use crate::hardware::expert_offloading::direct_io::AlignedBuffer;

/// Default capacity for each staging slot (64 MB).
pub const DEFAULT_STAGING_SLOT_BYTES: usize = 64 * 1024 * 1024;

/// Metadata for an expert cached in a staging slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedExpertMeta {
    pub layer: usize,
    pub expert_id: usize,
    pub offset_in_slot: usize,
    pub length: usize,
}

/// A fixed static double-buffer containing two 4KB-aligned memory slots.
pub struct StaticExpertStagingBuffer {
    /// Slot A: 4KB sector aligned buffer
    slot_a: AlignedBuffer,
    /// Slot B: 4KB sector aligned buffer
    slot_b: AlignedBuffer,
    /// Index of the active slot (0 = Slot A, 1 = Slot B)
    active_slot_idx: usize,
    /// Metadata for experts currently loaded in Slot A
    meta_a: Vec<StagedExpertMeta>,
    /// Metadata for experts currently loaded in Slot B
    meta_b: Vec<StagedExpertMeta>,
    /// Current write offset in the prefetch slot
    prefetch_write_cursor: usize,
}

impl StaticExpertStagingBuffer {
    /// Creates a new static staging double-buffer with given per-slot capacity in bytes.
    pub fn new(slot_capacity_bytes: usize) -> anyhow::Result<Self> {
        let capacity = if slot_capacity_bytes == 0 {
            DEFAULT_STAGING_SLOT_BYTES
        } else {
            slot_capacity_bytes
        };

        info!(
            "🔄 [RingBuffer] Allocating Static Double-Buffer: 2 x {:.2} MB (Total: {:.2} MB)",
            capacity as f64 / (1024.0 * 1024.0),
            (capacity * 2) as f64 / (1024.0 * 1024.0)
        );

        let slot_a = AlignedBuffer::new(capacity)?;
        let slot_b = AlignedBuffer::new(capacity)?;

        Ok(Self {
            slot_a,
            slot_b,
            active_slot_idx: 0,
            meta_a: Vec::with_capacity(16),
            meta_b: Vec::with_capacity(16),
            prefetch_write_cursor: 0,
        })
    }

    /// Swaps the active slot and prefetch slot (Ping-Pong switch).
    /// Clears the prefetch slot metadata for the next incoming layer.
    pub fn swap_slots(&mut self) {
        self.active_slot_idx = 1 - self.active_slot_idx;
        self.prefetch_write_cursor = 0;
        if self.active_slot_idx == 0 {
            self.meta_b.clear();
        } else {
            self.meta_a.clear();
        }
        debug!(
            "🔄 [RingBuffer] Swapped slots: Active is now Slot {}",
            if self.active_slot_idx == 0 { "A" } else { "B" }
        );
    }

    /// Returns a slice to the currently active computation buffer.
    pub fn active_buffer(&self) -> &[u8] {
        if self.active_slot_idx == 0 {
            &self.slot_a
        } else {
            &self.slot_b
        }
    }

    /// Returns a mutable slice to the background prefetch buffer.
    pub fn prefetch_buffer_mut(&mut self) -> &mut [u8] {
        if self.active_slot_idx == 0 {
            &mut self.slot_b
        } else {
            &mut self.slot_a
        }
    }

    /// Stages an expert into the prefetch slot buffer.
    /// Returns the byte offset in the prefetch buffer where the expert was placed.
    pub fn stage_expert(
        &mut self,
        layer: usize,
        expert_id: usize,
        expert_bytes: &[u8],
    ) -> anyhow::Result<usize> {
        let expert_len = expert_bytes.len();
        let slot_len = if self.active_slot_idx == 0 {
            self.slot_b.len()
        } else {
            self.slot_a.len()
        };

        if self.prefetch_write_cursor + expert_len > slot_len {
            anyhow::bail!(
                "Ring buffer prefetch slot overflow: cursor {} + expert {} > capacity {}",
                self.prefetch_write_cursor,
                expert_len,
                slot_len
            );
        }

        let start_offset = self.prefetch_write_cursor;
        let prefetch_buf = self.prefetch_buffer_mut();
        prefetch_buf[start_offset..start_offset + expert_len].copy_from_slice(expert_bytes);

        let meta = StagedExpertMeta {
            layer,
            expert_id,
            offset_in_slot: start_offset,
            length: expert_len,
        };

        if self.active_slot_idx == 0 {
            self.meta_b.push(meta);
        } else {
            self.meta_a.push(meta);
        }

        self.prefetch_write_cursor += expert_len;
        Ok(start_offset)
    }

    /// Looks up whether an expert is currently staged in the active buffer.
    pub fn lookup_active(&self, layer: usize, expert_id: usize) -> Option<StagedExpertMeta> {
        let meta_list = if self.active_slot_idx == 0 {
            &self.meta_a
        } else {
            &self.meta_b
        };

        meta_list.iter().find(|m| m.layer == layer && m.expert_id == expert_id).copied()
    }

    /// Returns the capacity of a single slot in bytes.
    pub fn slot_capacity(&self) -> usize {
        self.slot_a.len()
    }
}

/// Thread-safe shared wrapper for StaticExpertStagingBuffer.
#[derive(Clone)]
pub struct SharedStagingBuffer(pub Arc<Mutex<StaticExpertStagingBuffer>>);

impl SharedStagingBuffer {
    pub fn new(slot_capacity_bytes: usize) -> anyhow::Result<Self> {
        Ok(Self(Arc::new(Mutex::new(StaticExpertStagingBuffer::new(slot_capacity_bytes)?))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_staging_and_swap() {
        let mut ring = StaticExpertStagingBuffer::new(4096 * 4).unwrap();
        assert_eq!(ring.active_slot_idx, 0);

        // Stage into prefetch slot (Slot B)
        let dummy_expert_data = vec![42u8; 128];
        let offset = ring.stage_expert(1, 7, &dummy_expert_data).unwrap();
        assert_eq!(offset, 0);

        // It should NOT be active yet in Slot A
        assert!(ring.lookup_active(1, 7).is_none());

        // Perform swap
        ring.swap_slots();
        assert_eq!(ring.active_slot_idx, 1);

        // Now it SHOULD be active in Slot B
        let active_meta = ring.lookup_active(1, 7);
        assert!(active_meta.is_some());
        let meta = active_meta.unwrap();
        assert_eq!(meta.layer, 1);
        assert_eq!(meta.expert_id, 7);
        assert_eq!(meta.length, 128);

        // Verify data in active buffer
        let active_buf = ring.active_buffer();
        assert_eq!(&active_buf[0..128], &dummy_expert_data[..]);
    }
}
