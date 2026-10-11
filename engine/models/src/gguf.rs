//! 📐 Model geometry read from the GGUF header, and the KV / weight byte math llama.cpp uses.
//! Every number here comes from the file: metadata keys, per-layer arrays, and tensor byte sizes.

use serde::{Deserialize, Serialize};

/// llama.cpp pads KV cells to this many (llama-kv-cache: GGML_PAD(..., 256)).
pub const KV_CELL_PAD: u32 = 256;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelGeometry {
    pub architecture: String,
    pub context_length: u32,
    pub embedding_length: u32,
    pub block_count: u32,
    pub head_count: u32,
    /// First layer's KV head count (for display); per-layer values are in `head_count_kv_per_layer`
    pub head_count_kv: u32,
    #[serde(default)]
    pub head_count_kv_per_layer: Vec<u32>,
    /// K / V head size of full-attention layers (`<arch>.attention.key_length` / `value_length`)
    #[serde(default)]
    pub key_length: u32,
    #[serde(default)]
    pub value_length: u32,
    /// K / V head size of sliding-window layers (`key_length_swa` / `value_length_swa`, else same as full)
    #[serde(default)]
    pub key_length_swa: u32,
    #[serde(default)]
    pub value_length_swa: u32,
    /// Sliding window size in tokens, when the model has one
    pub sliding_window: Option<u32>,
    /// true = layer uses the sliding window (`<arch>.attention.sliding_window_pattern`)
    #[serde(default)]
    pub swa_layers: Vec<bool>,
    /// Last N layers reuse earlier layers' KV and allocate none (`<arch>.attention.shared_kv_layers`)
    #[serde(default)]
    pub shared_kv_layers: u32,
    pub expert_count: Option<u32>,
    pub expert_used_count: Option<u32>,
    pub file_size_bytes: u64,
    /// Vocabulary size from the token embedding tensor shape (0 = unknown).
    #[serde(default)]
    pub vocab_size: u32,
    #[serde(default)]
    pub weights: WeightBytes,
}

/// Tensor bytes grouped by where llama.cpp places them.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WeightBytes {
    /// All tensor data
    pub total: u64,
    /// Input embeddings (`token_embd`, `per_layer_token_embd`): llama.cpp always keeps them on CPU
    pub input_cpu: u64,
    /// Output layer tensors (outside blocks, not input), placed with the output head
    pub output: u64,
    /// Output head shares `token_embd` (no `output.weight`): llama.cpp loads a copy on the output device
    pub tied_output_copy: u64,
    /// Non-expert bytes of each block
    pub layer: Vec<u64>,
    /// Routed-expert bytes of each block (`ffn_*_exps`, what `--n-cpu-moe` moves to CPU)
    pub layer_experts: Vec<u64>,
}

impl WeightBytes {
    pub fn experts_total(&self) -> u64 {
        self.layer_experts.iter().sum()
    }

    /// Bytes on the GPU when every block and the output head are offloaded.
    pub fn full_gpu(&self) -> u64 {
        self.total.saturating_sub(self.input_cpu) + self.tied_output_copy
    }

    /// Output head bytes on the GPU (output tensors + tied copy).
    pub fn output_gpu(&self) -> u64 {
        self.output + self.tied_output_copy
    }
}

impl ModelGeometry {
    fn kv_heads(&self, layer: usize) -> u64 {
        self.head_count_kv_per_layer.get(layer).copied().unwrap_or(self.head_count_kv) as u64
    }

    fn is_swa(&self, layer: usize) -> bool {
        self.sliding_window.unwrap_or(0) > 0 && self.swa_layers.get(layer).copied().unwrap_or(false)
    }

    /// Head size used for display (full-attention K head size).
    pub fn head_dim(&self) -> u32 {
        self.key_length
    }

    /// Layers that allocate their own KV cache (shared-KV layers at the end allocate none).
    pub fn kv_layer_count(&self) -> usize {
        self.block_count.saturating_sub(self.shared_kv_layers) as usize
    }

    /// KV cache bytes of one layer, the way llama.cpp sizes it: full layers hold the padded context,
    /// sliding-window layers hold min(context, window + ubatch), both padded to 256 cells.
    pub fn kv_layer_bytes(&self, layer: usize, n_ctx: u32, quant: cluaiz_core::KvCacheQuantization, ubatch: u32) -> u64 {
        if layer >= self.kv_layer_count() {
            return 0;
        }
        let pad = |n: u32| n.div_ceil(KV_CELL_PAD) * KV_CELL_PAD;
        let full_cells = pad(n_ctx) as u64;
        let (cells, k_dim, v_dim) = if self.is_swa(layer) {
            let swa_cells = pad(self.sliding_window.unwrap_or(0) + ubatch) as u64;
            (full_cells.min(swa_cells), self.key_length_swa as u64, self.value_length_swa as u64)
        } else {
            (full_cells, self.key_length as u64, self.value_length as u64)
        };
        let elements = self.kv_heads(layer) * (k_dim + v_dim) * cells;
        (elements as f64 * quant.bytes_per_element()).ceil() as u64
    }

    /// Total KV cache bytes for `n_ctx` tokens.
    pub fn kv_cache_bytes(&self, n_ctx: u32, quant: cluaiz_core::KvCacheQuantization, ubatch: u32) -> u64 {
        (0..self.kv_layer_count()).map(|l| self.kv_layer_bytes(l, n_ctx, quant, ubatch)).sum()
    }

    /// Logits part of llama.cpp's compute buffer: the worst-case prompt graph is reserved with
    /// min(ubatch, n_outputs_max) outputs (llama-context.cpp graph reserve), i.e. vocab x ubatch x f32.
    /// Returns 0 when the vocabulary size is unknown, so callers must not treat it as measured.
    pub fn logits_buffer_bytes(&self, ubatch: u32) -> u64 {
        self.vocab_size as u64 * ubatch as u64 * 4
    }

    pub fn is_moe(&self) -> bool {
        self.weights.experts_total() > 0
    }
}

pub use crate::prober::GgufProber;
