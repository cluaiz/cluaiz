//! Synchronous native parameter translation for the existing chat load owner.
use cluaiz_core::ActiveUserSettings;
use cluaiz_hw::SystemInfo;
use cluaiz_models::types::StructuralDNA;
use cluaiz_plan::ExecutionPlan;
use llama_engine::NativeLlama;

/// Live hardware for planning a GGUF model. Without a loaded GPU backend (no GPU pack, or its driver
/// is missing) llama.cpp has no GPU to place layers on, so the plan sees none.
pub(crate) fn gguf_hardware() -> SystemInfo {
    let mut hw = cluaiz_hw::HardwareProber::with_live_memory();
    if !llama_engine::gpu_offload_available() {
        hw.gpus.clear();
    }
    hw
}

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
    let mtp = if embedding { 0 } else { mtp_mode(model_path, plan, settings) };
    let loaded = NativeLlama::load(model_path, model_params, ctx_params, dna, 0, 0, mtp, None);
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
    /// llama.cpp MoE cache bytes (0 = off) and the CPU share of its misses
    pub moe_cache_bytes: u64,
    pub moe_cache_cpu_share: f32,
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
            moe_cache_bytes: plan.moe_cache_bytes,
            moe_cache_cpu_share: plan.moe_cache_cpu_share,
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
    // Sliding-window layers keep only their window (+ a batch) of cells, as llama-server and the
    // planner's KV size (`ModelGeometry::kv_layer_bytes`) assume. llama.cpp's context default is
    // the full context for them: on Gemma 4 26B at 4096 tokens, Q8 KV, that is 479 MiB of KV
    // instead of 213. Prompts that rewrite older turns restart from a checkpoint (`checkpoints`).
    ctx_params.swa_full = 0;
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
    // llama.cpp's MoE cache for the experts in RAM (cluaiz_plan::moe_cache), off unless the plan set it
    if n_gpu_layers != 0 {
        ctx_params.moe_cache_size = placement.moe_cache_bytes as usize;
        ctx_params.moe_cache_cpu_share = placement.moe_cache_cpu_share;
    }
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

/// MTP for this load: 0 = off, 1 = on, 2 = on with the per-answer guard (llama `stream.rs`, "MTP
/// guard"). A model without MTP layers loads without it whatever this says. Auto follows run 23
/// (`test/cloud/runs/2026-11-20-run23-mtp-placements`), first rule that applies:
/// 1. experts read from the disk: off (every prompt was slower, 0.63-0.98x);
/// 2. a `cluaiz bench --mtp-compare` result for this model and placement: on when generation was at
///    least 5% faster ([`cluaiz_plan::speed::FASTER_BY`]), else off;
/// 3. otherwise on with the guard, which stops guessing in an answer where it is slower (prose on a
///    dense model, MoE experts in RAM) and keeps it where it gains (JSON, code, MoE on the GPU).
fn mtp_mode(model_path: &str, plan: &ExecutionPlan, settings: &ActiveUserSettings) -> u8 {
    use cluaiz_core::FeatureState;
    let (mode, why) = match settings.optimization.mtp {
        FeatureState::Off => return 0,
        FeatureState::On => return 1,
        FeatureState::Auto => mtp_auto(model_path, plan, settings),
    };
    tracing::info!("🔮 [MTP] Auto: {} ({})", ["off", "on", "on with the per-answer guard"][mode as usize], why);
    mode
}

fn mtp_auto(model_path: &str, plan: &ExecutionPlan, settings: &ActiveUserSettings) -> (u8, &'static str) {
    use cluaiz_plan::{speed, PlacementTier};
    if plan.disk_stream_gb > 0.0 || plan.selected_tier == PlacementTier::SsdStreaming {
        return (0, "weights are read from the disk, where MTP was slower on every prompt");
    }
    let Ok(geometry) = cluaiz_models::GgufProber::probe_geometry(std::path::Path::new(model_path)) else {
        return (0, "the model file could not be read for the MTP rules");
    };
    // MTP's memory is not in the plan yet: off when it would not fit next to the planned placement.
    // Run 23 measured +0.11-0.19 GB (dense 2B, 0.05 GB per block) and +0.45-0.71 GB (MoE 35B-A3B,
    // 0.44 GB per block): about 1.6 blocks of weights plus 0.15 GB of context and buffers.
    let block_gb = geometry.file_size_bytes as f64 / 1e9 / geometry.block_count.max(1) as f64;
    let mtp_gb = 1.6 * block_gb + 0.15;
    let headroom = if plan.n_gpu_layers != 0 {
        plan.vram_free_gb - plan.vram_buffer_gb - plan.vram_budget_gb
    } else {
        plan.ram_budget_limit_gb - plan.ram_budget_gb
    };
    if headroom < mtp_gb {
        return (0, "its memory would not fit next to this placement");
    }
    let signature = speed::Signature {
        tier: plan.selected_tier,
        n_gpu_layers: plan.n_gpu_layers,
        n_cpu_moe: plan.n_cpu_moe,
        kv_offload: plan.kv_offload,
        moe_cache: plan.moe_cache_bytes > 0,
    };
    let measured = |test| speed::measured(test, &geometry, settings, plan.context_window, plan.kv_cache_type, &signature);
    match (measured(speed::Test::Text), measured(speed::Test::TextMtp)) {
        (Some(off), Some(on)) if on >= off * speed::FASTER_BY => (1, "measured faster on this placement"),
        (Some(_), Some(_)) => (0, "measured slower on this placement"),
        _ => (2, "no measurement for this placement"),
    }
}
