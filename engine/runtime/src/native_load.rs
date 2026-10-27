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
    load_native(model_path, plan, settings, hw, dna, false)
}

/// Same placement/thread/parameter owner, with whole-sequence embedding output enabled.
pub(crate) fn load_embedding_with_plan(
    model_path: &str,
    plan: &ExecutionPlan,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
) -> anyhow::Result<llama_engine::native::embedding::EmbeddingModel> {
    llama_engine::native::embedding::EmbeddingModel::new(load_native(
        model_path,
        plan,
        settings,
        hw,
        &mut StructuralDNA::default(),
        true,
    )?)
}

fn load_native(
    model_path: &str,
    plan: &ExecutionPlan,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
    dna: &mut StructuralDNA,
    embedding: bool,
) -> anyhow::Result<NativeLlama> {
    let placement = Placement::of(plan, settings);
    let (model_params, mut ctx_params, moe_overrides) = native_params(&placement, settings, hw);
    if embedding {
        ctx_params.embeddings = 1;
        // Non-causal pooling must evaluate the entire sequence in one physical batch.
        ctx_params.n_batch = plan.context_window;
        ctx_params.n_ubatch = plan.context_window;
    }
    // llama.cpp only reads the override list during load; `moe_overrides` lives until this returns
    let loaded = NativeLlama::load(model_path, model_params, ctx_params, dna, 0, 0, 0, None);
    drop(moe_overrides);
    loaded
}

/// What a placement sets in llama.cpp's parameters
pub(crate) struct Placement {
    pub n_gpu_layers: i32,
    pub n_cpu_moe: usize,
    pub use_mmap: bool,
    pub disk_stream: bool,
    pub n_ctx: u32,
    pub kv: cluaiz_core::KvCacheQuantization,
    pub kv_offload: bool,
    pub op_offload: bool,
}

impl Placement {
    fn of(plan: &ExecutionPlan, settings: &ActiveUserSettings) -> Self {
        Self {
            n_gpu_layers: plan.n_gpu_layers,
            n_cpu_moe: plan.n_cpu_moe as usize,
            use_mmap: plan.use_mmap(settings),
            disk_stream: plan.disk_stream_gb > 0.0,
            n_ctx: plan.context_window,
            kv: plan.kv_cache_type,
            kv_offload: plan.kv_offload,
            op_offload: plan.op_offload,
        }
    }
}

/// The llama.cpp model/context params of a placement (one owner for loads and dry runs). The
/// returned override list must outlive every use of the model params.
fn native_params(
    placement: &Placement,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
) -> (
    llama_engine::ffi::llama_cpp::LlamaModelParams,
    llama_engine::ffi::llama_cpp::LlamaContextParams,
    llama_engine::moe_override::CpuMoeOverrides,
) {
    let n_gpu_layers = placement.n_gpu_layers;
    let (type_k, type_v) = placement.kv.to_ggml_types();
    // SAFETY: native default-parameter constructors take no handles or borrowed inputs.
    let (mut model_params, mut ctx_params) = unsafe {
        (
            llama_engine::ffi::llama_cpp::llama_model_default_params(),
            llama_engine::ffi::llama_cpp::llama_context_default_params(),
        )
    };
    model_params.n_gpu_layers = n_gpu_layers;
    if n_gpu_layers == 0 {
        model_params.set_cpu_only();
    }
    // STEP 3: MoE over the RAM limit reads experts from disk -> mmap on, CPU repack off
    model_params.set_mmap(placement.use_mmap);
    if placement.disk_stream {
        model_params.use_extra_bufts = false;
    }
    // MoE split: experts of the first n_cpu_moe blocks in RAM (llama.cpp --n-cpu-moe)
    let moe_overrides = llama_engine::moe_override::CpuMoeOverrides::new(placement.n_cpu_moe);
    model_params.tensor_buft_overrides = moe_overrides.as_ptr();

    ctx_params.n_ctx = placement.n_ctx;
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
    ctx_params.offload_kqv = if n_gpu_layers != 0 && placement.kv_offload {
        1
    } else {
        0
    };
    ctx_params.op_offload = if n_gpu_layers != 0 && placement.op_offload {
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
    (model_params, ctx_params, moe_overrides)
}

/// llama.cpp's dry run (no allocation) of a placement: bytes per device
pub(crate) fn project(
    model_path: &str,
    placement: &Placement,
    settings: &ActiveUserSettings,
    hw: &SystemInfo,
) -> anyhow::Result<llama_engine::native::projection::ProjectedMemory> {
    let (model_params, ctx_params, moe_overrides) = native_params(placement, settings, hw);
    let projected = llama_engine::native::projection::project(model_path, &model_params, &ctx_params);
    drop(moe_overrides);
    projected
}
