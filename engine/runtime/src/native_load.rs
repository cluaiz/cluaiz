//! Synchronous native parameter translation for the existing chat load owner.
use cluaiz_core::ActiveUserSettings;
use cluaiz_hw::SystemInfo;
use cluaiz_models::types::StructuralDNA;
use cluaiz_plan::ExecutionPlan;
use llama_engine::NativeLlama;

/// Builds the llama.cpp model/context params from the plan and loads the model.
/// Synchronous on purpose: the params and the MoE override list hold raw C pointers,
/// which must not live inside an async state machine.
pub(super) fn load_with_plan(
    model_path: &str,
    plan: &ExecutionPlan,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
    dna: &mut StructuralDNA,
) -> anyhow::Result<NativeLlama> {
    let n_gpu_layers = plan.n_gpu_layers;
    let (type_k, type_v) = plan.kv_cache_type.to_ggml_types();
    // SAFETY: native default-parameter constructors take no handles or borrowed inputs.
    let (mut model_params, mut ctx_params) = unsafe {
        (
            llama_engine::ffi::llama_cpp::llama_model_default_params(),
            llama_engine::ffi::llama_cpp::llama_context_default_params(),
        )
    };
    model_params.n_gpu_layers = n_gpu_layers;
    // STEP 3: MoE over the RAM limit reads experts from disk -> mmap on, CPU repack off
    model_params.set_mmap(plan.use_mmap(settings));
    if plan.disk_stream_gb > 0.0 {
        model_params.use_extra_bufts = false;
    }
    // MoE split: experts of the first n_cpu_moe blocks in RAM (llama.cpp --n-cpu-moe)
    let moe_overrides = llama_engine::moe_override::CpuMoeOverrides::new(plan.n_cpu_moe as usize);
    model_params.tensor_buft_overrides = moe_overrides.as_ptr();

    ctx_params.n_ctx = plan.context_window;
    ctx_params.n_batch = settings.optimization.resolved_batch_size();
    ctx_params.n_ubatch = settings.optimization.resolved_ubatch_size();
    ctx_params.type_k = type_k;
    ctx_params.type_v = type_v;
    // llama.cpp flash_attn_type: -1 = auto (llama.cpp checks the device), 0 = off, 1 = on
    ctx_params.flash_attn_type = match settings.optimization.flash_attention {
        cluaiz_core::FeatureState::On => 1,
        cluaiz_core::FeatureState::Off => 0,
        cluaiz_core::FeatureState::Auto => -1,
    };
    // KV next to its GPU layers, except CTX rule 5B (weights on GPU, whole KV in RAM)
    ctx_params.offload_kqv = if n_gpu_layers != 0 && plan.kv_offload {
        1
    } else {
        0
    };
    ctx_params.op_offload = if n_gpu_layers != 0 && plan.op_offload {
        1
    } else {
        0
    };
    // Same default as llama.cpp's CLI (physical cores); llama_context_default_params alone gives 4
    let (n_threads, n_threads_batch) = settings
        .optimization
        .resolved_threads(hw.cpu.physical_cores);
    ctx_params.n_threads = n_threads;
    ctx_params.n_threads_batch = n_threads_batch;
    ctx_params.no_perf = 0; // keep llama.cpp's timing counters for the speed report

    // llama.cpp only reads the override list during load; `moe_overrides` lives until this returns
    NativeLlama::load(model_path, model_params, ctx_params, dna, 0, 0, 0, None)
}
