//! MoE expert override: keep the experts of the first N blocks in RAM (CPU), everything else of those
//! blocks (attention, shared weights, KV) on the GPU. Same as llama.cpp `--n-cpu-moe N`
//! (common/common.h: pattern `blk\.{i}\.ffn_(up|down|gate|gate_up)_(ch|)exps` -> CPU buffer).

use crate::ffi::llama_cpp::{ggml_backend_cpu_buffer_type, LlamaModelTensorBuftOverride};
use std::ffi::CString;

/// Owns the C strings and the null-terminated override list.
/// Keep it alive until `llama_model_load_from_file` has returned; llama.cpp only reads it during load.
pub struct CpuMoeOverrides {
    _patterns: Vec<CString>,
    list: Vec<LlamaModelTensorBuftOverride>,
}

impl CpuMoeOverrides {
    pub fn new(n_cpu_moe: usize) -> Self {
        let cpu = unsafe { ggml_backend_cpu_buffer_type() };
        let patterns: Vec<CString> = (0..n_cpu_moe)
            .map(|i| CString::new(format!(r"blk\.{}\.ffn_(up|down|gate|gate_up)_(ch|)exps", i)).expect("no NUL"))
            .collect();
        let mut list: Vec<LlamaModelTensorBuftOverride> = patterns
            .iter()
            .map(|p| LlamaModelTensorBuftOverride { pattern: p.as_ptr(), buft: cpu })
            .collect();
        list.push(LlamaModelTensorBuftOverride { pattern: std::ptr::null(), buft: std::ptr::null() });
        Self { _patterns: patterns, list }
    }

    pub fn is_empty(&self) -> bool {
        self.list.len() <= 1
    }

    /// Pointer for `LlamaModelParams::tensor_buft_overrides`.
    pub fn as_ptr(&self) -> *const LlamaModelTensorBuftOverride {
        self.list.as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn patterns_match_llama_cpp_n_cpu_moe() {
        let o = CpuMoeOverrides::new(3);
        assert!(!o.is_empty());
        let first = unsafe { CStr::from_ptr(o.list[0].pattern) }.to_str().unwrap().to_string();
        assert_eq!(first, r"blk\.0\.ffn_(up|down|gate|gate_up)_(ch|)exps");
        assert!(o.list[3].pattern.is_null());
        assert!(CpuMoeOverrides::new(0).is_empty());
    }
}
