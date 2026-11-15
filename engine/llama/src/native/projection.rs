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

#[cfg(test)]
mod layout_tests {
    use crate::ffi::llama_cpp::*;
    use crate::ffi::mtmd::{mtmd_gen_audio_info, mtmd_helper_gen_audio_inp};
    use std::mem::{offset_of, size_of};

    unsafe extern "C" {
        fn cluaiz_param_layout(out: *mut u64);
    }

    /// The Rust copies of llama.h / mtmd-helper.h structs must match the C layout exactly: a field
    /// added or removed upstream would otherwise shift every value after it
    #[test]
    fn ffi_structs_match_the_c_headers() {
        let mut c = [0u64; 20];
        // SAFETY: the bridge writes twenty values into the array.
        unsafe { cluaiz_param_layout(c.as_mut_ptr()) };
        let rust = [
            ("llama_context_params size", size_of::<LlamaContextParams>()),
            ("llama_context_params.type_v", offset_of!(LlamaContextParams, type_v)),
            ("llama_context_params.abort_callback", offset_of!(LlamaContextParams, abort_callback)),
            ("llama_context_params.swa_full", offset_of!(LlamaContextParams, swa_full)),
            ("llama_context_params.ctx_other", offset_of!(LlamaContextParams, ctx_other)),
            ("llama_model_params size", size_of::<LlamaModelParams>()),
            ("llama_model_params.n_gpu_layers", offset_of!(LlamaModelParams, n_gpu_layers)),
            ("llama_model_params.kv_overrides", offset_of!(LlamaModelParams, kv_overrides)),
            ("llama_model_params.load_mtp", offset_of!(LlamaModelParams, load_mtp)),
            ("llama_perf_context_data size", size_of::<LlamaPerfContextData>()),
            ("llama_chat_message size", size_of::<LlamaChatMessage>()),
            ("llama_token_data size", size_of::<LlamaTokenData>()),
            ("llama_token_data_array size", size_of::<LlamaTokenDataArray>()),
            ("llama_sampler_chain_params size", size_of::<LlamaSamplerChainParams>()),
            ("llama_batch size", size_of::<LlamaBatch>()),
            ("llama_model_tensor_buft_override size", size_of::<LlamaModelTensorBuftOverride>()),
            ("mtmd_helper_gen_audio_inp size", size_of::<mtmd_helper_gen_audio_inp>()),
            ("mtmd_helper_gen_audio_inp.speaker_ref", offset_of!(mtmd_helper_gen_audio_inp, speaker_ref)),
            ("mtmd_helper_gen_audio_inp.out_type", offset_of!(mtmd_helper_gen_audio_inp, out_type)),
            ("mtmd_gen_audio_info size", size_of::<mtmd_gen_audio_info>()),
        ];
        let wrong: Vec<String> = rust
            .iter()
            .zip(c)
            .filter(|((_, r), c)| *r as u64 != *c)
            .map(|((name, r), c)| format!("{name}: Rust {r}, C {c}"))
            .collect();
        assert!(wrong.is_empty(), "C headers changed, update ffi/llama_cpp.rs or ffi/mtmd.rs: {wrong:?}");
    }
}
