//! ⚙️ ONNX Metadata Headers & Configuration Loader
//! Reads onnx_metadata_headers.json from config directory or uses typed defaults.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OnnxMetadataHeaders {
    #[serde(default = "default_n_gpu_layers")]
    pub n_gpu_layers: i32,
    #[serde(default = "default_n_ctx")]
    pub n_ctx: i32,
    #[serde(default)]
    pub intra_op_num_threads: usize,
    #[serde(default = "default_graph_opt")]
    pub graph_optimization_level: String,
    #[serde(default)]
    pub enable_profiling: bool,
    #[serde(default)]
    pub inter_op_num_threads: usize,
    #[serde(default = "default_true")]
    pub enable_mem_pattern: bool,
    #[serde(default = "default_true")]
    pub enable_cpu_mem_arena: bool,
    #[serde(default = "default_exec_mode")]
    pub execution_mode: String,
    #[serde(default)]
    pub gpu_mem_limit_bytes: usize,
    #[serde(default = "default_arena_strat")]
    pub arena_extend_strategy: String,
    #[serde(default = "default_true")]
    pub enable_ort_transformers_optimization: bool,
    #[serde(default = "default_kv_type")]
    pub kv_cache_data_type: String,
    #[serde(default)]
    pub use_deterministic_compute: bool,
}

fn default_n_ctx() -> i32 { 0 }
fn default_n_gpu_layers() -> i32 { -1 }
fn default_graph_opt() -> String { "ORT_ENABLE_ALL".to_string() }
fn default_true() -> bool { true }
fn default_exec_mode() -> String { "ORT_SEQUENTIAL".to_string() }
fn default_arena_strat() -> String { "kNextPowerOfTwo".to_string() }
fn default_kv_type() -> String { "ort_fp16".to_string() }

impl Default for OnnxMetadataHeaders {
    fn default() -> Self {
        Self {
            n_gpu_layers: -1,
            n_ctx: 0,
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
        }
    }
}

impl OnnxMetadataHeaders {
    pub fn load() -> Self {
        let path = cluaiz_core::EnvironmentManager::current().config_dir().join("onnx_metadata_headers.json");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str(&content) {
                    return meta;
                }
            }
        }
        Self::default()
    }
}
