use serde::{Deserialize, Serialize};
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use crate::define_config;

#[derive(Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize, Clone, Debug)]
pub struct OnnxMetadataHeaders {
    #[serde(default = "default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    pub intra_op_num_threads: usize,
    pub graph_optimization_level: String,
    pub enable_profiling: bool,
    
    // Memory & Hardware Control Features
    pub inter_op_num_threads: usize,
    pub enable_mem_pattern: bool,
    pub enable_cpu_mem_arena: bool,
    pub execution_mode: String,
    pub gpu_mem_limit_bytes: usize,
    pub arena_extend_strategy: String,
    
    pub enable_ort_transformers_optimization: bool,
    pub kv_cache_data_type: String,
    pub use_deterministic_compute: bool,

    pub user_moved_flags: crate::hardware::schema::gguf_metadata::UserMovedFlags,
}

fn default_n_gpu_layers() -> i32 {
    -1
}

impl Default for OnnxMetadataHeaders {
    fn default() -> Self {
        Self {
            n_gpu_layers: -1,
            intra_op_num_threads: 0,
            graph_optimization_level: "ORT_ENABLE_ALL".to_string(),
            enable_profiling: false,
            inter_op_num_threads: 0,
            enable_mem_pattern: true,
            enable_cpu_mem_arena: true,
            execution_mode: "ORT_SEQUENTIAL".to_string(),
            gpu_mem_limit_bytes: 0,
            arena_extend_strategy: "kNextPowerOfTwo".to_string(),
            enable_ort_transformers_optimization: true,
            kv_cache_data_type: "ort_fp16".to_string(),
            use_deterministic_compute: false,
            user_moved_flags: crate::hardware::schema::gguf_metadata::UserMovedFlags::default(),
        }
    }
}

// Generate the fast zero-copy load/save functions using the macro!
define_config!(OnnxMetadataHeaders, "onnx_metadata_headers");
