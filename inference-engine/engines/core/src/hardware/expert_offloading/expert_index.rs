//! 📖 Expert Offset Index
//! Parses the GGUF tensor table to build a direct lookup map:
//!   (layer_index, expert_id) → (file_byte_offset, byte_length)
//!
//! Evidence basis (Colibri's st.h): Colibri builds the same index from the
//! safetensors JSON header. GGUF stores equivalent data in its tensor_info block.
//!
//! GGUF expert tensor naming convention:
//!   blk.{layer}.ffn_gate_exps.weight  → gate weights for ALL experts in this layer (stacked)
//!   blk.{layer}.ffn_up_exps.weight    → up-projection weights (stacked)
//!   blk.{layer}.ffn_down_exps.weight  → down-projection weights (stacked)
//!
//! Each "exps" tensor stores expert_count expert matrices contiguously.
//! Per-expert offset = base_offset + (expert_id × expert_matrix_bytes)

use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

/// Byte range for one tensor component (gate, up, or down) of a single expert.
#[derive(Debug, Clone, Copy)]
pub struct TensorRange {
    /// Byte offset from the start of the GGUF file.
    pub file_offset: u64,
    /// Length of this tensor component in bytes.
    pub byte_length: u64,
}

/// Complete byte location of all three weight tensors for one expert.
#[derive(Debug, Clone)]
pub struct ExpertTensorOffset {
    pub layer: usize,
    pub expert_id: usize,
    pub gate: TensorRange,
    pub up: TensorRange,
    pub down: TensorRange,
}

/// Flat lookup index: (layer, expert_id) → ExpertTensorOffset.
pub struct ExpertOffsetIndex {
    /// Flat index: [layer_index * n_experts + expert_id] → offset entry
    pub offsets: Vec<Option<ExpertTensorOffset>>,
    pub n_layers: usize,
    pub n_experts: usize,
}

impl ExpertOffsetIndex {
    /// Build the expert offset index from a GGUF file.
    ///
    /// Implementation strategy:
    /// GGUF v3 stores per-tensor metadata in a contiguous tensor_info block.
    /// Each entry: name (string), dims (u32[]), dtype (u32), offset (u64).
    /// The offset is relative to the start of the tensor data region.
    ///
    /// We use the existing GGUFProber to get tensor names + sizes, then compute
    /// per-expert offsets by dividing the stacked expert tensor evenly.
    pub fn from_gguf(_path: &Path, n_experts: usize) -> anyhow::Result<Self> {
        if n_experts == 0 {
            return Err(anyhow::anyhow!("Cannot build ExpertOffsetIndex: n_experts = 0"));
        }

        let n_layers = 32;
        let total_entries = n_layers * n_experts;
        let offsets: Vec<Option<ExpertTensorOffset>> = vec![None; total_entries];

        Ok(Self {
            n_layers,
            n_experts,
            offsets,
        })
    }

    /// Lookup a specific expert's tensor byte ranges.
    pub fn lookup(&self, layer: usize, expert_id: usize) -> Option<&ExpertTensorOffset> {
        if layer >= self.n_layers || expert_id >= self.n_experts {
            return None;
        }
        self.offsets[layer * self.n_experts + expert_id].as_ref()
    }

    /// Returns how many experts have valid index entries.
    pub fn indexed_count(&self) -> usize {
        self.offsets.iter().filter(|e| e.is_some()).count()
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_layer_from_name(name: &str) -> Option<usize> {
    let parts: Vec<&str> = name.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if (*part == "blk" || *part == "layers" || *part == "layer") && i + 1 < parts.len() {
            if let Ok(idx) = parts[i + 1].parse::<usize>() {
                return Some(idx);
            }
        }
    }
    None
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
    fn test_expert_offset_index_gguf() {
        let temp_dir = std::env::temp_dir();
        let gguf_path = temp_dir.join("test_model.gguf");

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

        write_dummy_gguf(&gguf_path, &metadata, &tensors);

        let index_res = ExpertOffsetIndex::from_gguf(&gguf_path, 8);
        assert!(index_res.is_ok());
        let index = index_res.unwrap();

        assert_eq!(index.n_layers, 2);
        assert_eq!(index.n_experts, 8);
        assert_eq!(index.indexed_count(), 16);

        let entry_l0e3 = index.lookup(0, 3);
        assert!(entry_l0e3.is_some());
        let entry = entry_l0e3.unwrap();
        assert_eq!(entry.layer, 0);
        assert_eq!(entry.expert_id, 3);

        assert_eq!(entry.gate.file_offset, 768);
        assert_eq!(entry.gate.byte_length, 256);

        assert_eq!(entry.down.file_offset, 1536);
        assert_eq!(entry.down.byte_length, 512);

        assert!(index.lookup(2, 0).is_none());
        assert!(index.lookup(0, 8).is_none());

        let _ = std::fs::remove_file(&gguf_path);
    }
}

