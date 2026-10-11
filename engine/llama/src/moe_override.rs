//! MoE expert override: keep the routed experts of the first N blocks in RAM (CPU buffer), everything
//! else of those blocks (attention, shared weights, KV) on the GPU. Same as llama.cpp `--n-cpu-moe N`
//! (common/common.h: `llm_ffn_block_regex(i, LLM_FFN_EXPS_REGEX)` -> `ggml_backend_cpu_buffer_type()`).

use crate::ffi::llama_cpp::{ggml_backend_cpu_buffer_type, LlamaModelTensorBuftOverride};
use std::ffi::CString;

/// llama.cpp `LLM_FFN_EXPS_REGEX`
const FFN_EXPS_REGEX: &str = r"\.ffn_(up|down|gate|gate_up)_(ch|)exps";

/// Owns the C strings and the null-terminated override list.
/// Keep it alive until `llama_model_load_from_file` has returned; llama.cpp only reads it during load.
pub struct CpuMoeOverrides {
    _patterns: Vec<CString>,
    list: Vec<LlamaModelTensorBuftOverride>,
}

impl CpuMoeOverrides {
    pub fn new(n_cpu_moe: usize) -> Self {
        let patterns: Vec<CString> = (0..n_cpu_moe)
            .map(|i| CString::new(format!(r"blk\.{}{}", i, FFN_EXPS_REGEX)).expect("pattern has no NUL"))
            .collect();
        let cpu = if patterns.is_empty() { std::ptr::null() } else { unsafe { ggml_backend_cpu_buffer_type() } };
        let mut list: Vec<LlamaModelTensorBuftOverride> = patterns
            .iter()
            .map(|p| LlamaModelTensorBuftOverride { pattern: p.as_ptr(), buft: cpu })
            .collect();
        // llama.cpp reads the list until a null pattern
        list.push(LlamaModelTensorBuftOverride { pattern: std::ptr::null(), buft: std::ptr::null() });
        Self { _patterns: patterns, list }
    }

    pub fn is_empty(&self) -> bool {
        self.list.len() <= 1
    }

    /// Pointer for `LlamaModelParams::tensor_buft_overrides` (null when there is nothing to override).
    pub fn as_ptr(&self) -> *const LlamaModelTensorBuftOverride {
        if self.is_empty() { std::ptr::null() } else { self.list.as_ptr() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn empty_override_list_is_null() {
        assert!(CpuMoeOverrides::new(0).as_ptr().is_null());
    }

    #[test]
    fn patterns_match_llama_cpp_n_cpu_moe() {
        // build only the strings; ggml_backend_cpu_buffer_type() needs the linked library
        let first = format!(r"blk\.{}{}", 0, FFN_EXPS_REGEX);
        assert_eq!(first, r"blk\.0\.ffn_(up|down|gate|gate_up)_(ch|)exps");
        let c = CString::new(first.clone()).unwrap();
        assert_eq!(unsafe { CStr::from_ptr(c.as_ptr()) }.to_str().unwrap(), first);
    }
}
