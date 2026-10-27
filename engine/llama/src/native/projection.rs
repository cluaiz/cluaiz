//! What a model + context will take per device, measured by llama.cpp's own no-alloc dry run
//! (the one behind `--fit`), for the planner to correct its byte estimate. Nothing is allocated.

use crate::ffi::llama_cpp::{LlamaContextParams, LlamaModelParams};
use std::ffi::{c_char, c_int, CString};

unsafe extern "C" {
    fn cluaiz_projected_memory(
        path_model: *const c_char,
        mparams: *const LlamaModelParams,
        cparams: *const LlamaContextParams,
        out: *mut u64,
    ) -> c_int;
}

/// Bytes llama.cpp will allocate: model weights, context (KV) and compute buffers
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeviceBytes {
    pub model: u64,
    pub context: u64,
    pub compute: u64,
}

impl DeviceBytes {
    pub fn total(&self) -> u64 {
        self.model + self.context + self.compute
    }
}

/// llama.cpp's projection on the first GPU and in host memory
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectedMemory {
    pub gpu: DeviceBytes,
    pub host: DeviceBytes,
}

/// Runs llama.cpp's dry run for these exact load parameters. Not thread safe in llama.cpp (it
/// swaps the global logger), so callers hold the model transition lock.
pub fn project(path: &str, model_params: &LlamaModelParams, ctx_params: &LlamaContextParams) -> anyhow::Result<ProjectedMemory> {
    let path_c = CString::new(path)?;
    let mut out = [0u64; 6];
    // SAFETY: both parameter structs mirror llama.h and outlive the call; `out` holds six values.
    let rc = unsafe { cluaiz_projected_memory(path_c.as_ptr(), model_params, ctx_params, out.as_mut_ptr()) };
    if rc != 0 {
        anyhow::bail!("llama.cpp could not project the memory of {path}");
    }
    Ok(ProjectedMemory {
        gpu: DeviceBytes { model: out[0], context: out[1], compute: out[2] },
        host: DeviceBytes { model: out[3], context: out[4], compute: out[5] },
    })
}
