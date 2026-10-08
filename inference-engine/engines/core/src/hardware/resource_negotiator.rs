//! Placement rules for GGUF models
//!
//! Measured at load time (nothing assumed):
//!   - free VRAM / free RAM, live, other apps included
//!   - bytes per device for weights + KV + compute buffer (llama.cpp dry run of the exact settings)
//!   - speed (short timed run: 64 prompt tokens + ~24 generated tokens)
//! Fixed numbers are named settings: VRAM buffer, RAM ceiling, context floor.
//!
//! STEP 1 - Full GPU check
//!   Do all layers + KV (at least the context floor) + compute buffer fit in free VRAM minus the VRAM buffer?
//!   Yes -> load everything on GPU, context per CTX rules. STOP, skip steps 2-4.
//!
//! STEP 2 - Split GPU + RAM
//!   GPU keeps as many layers as fit. MoE: attention/shared weights of every layer first, then expert groups.
//!   The rest goes to RAM. Check: RAM used now + Cluaiz RAM part <= RAM ceiling.
//!
//! STEP 3 - Bigger than VRAM + RAM ceiling
//!   MoE   -> allowed. Experts that do not fit stay on disk; the OS reads them when used (mmap, repack off).
//!   Dense -> not loaded. Message says how many GB are missing and what to change.
//!
//! STEP 4 - Who computes (speed check)
//!   Candidates: split plan(s) from step 2 + CPU only (+ CTX rule 5 variant).
//!   Load each candidate that fits the RAM ceiling, time it, keep the fastest generation.
//!   Within 3% noise, faster prompt wins. If none fits: least extra RAM, no speed check.
//!   Cached per model + GPU + RAM + settings. Measured again when any of these change.
//!
//! STEP 5 - Load the winner, print the log, write the real context for the UI.
//!
//! CTX RULES
//!   1. Custom N -> exactly N. Never cut.
//!   2. Max (-1) -> model's native context. Never cut.
//!   3. Auto (0) -> largest context (up to native) that keeps the model fully on GPU.
//!                  Not fully on GPU even at the floor -> floor (4096).
//!                  CPU only -> largest that fits under the RAM ceiling. Never below the floor.
//!   4. KV lives next to its layer: GPU layer -> KV in VRAM, RAM layer -> KV in RAM.
//!   5. Custom/Max so big that its KV pushes layers off the GPU -> two candidates, speed check picks:
//!        A) KV next to each layer, fewer layers on GPU
//!        B) all weights on GPU, whole KV in RAM (attention runs on CPU, slower as the chat grows)
//!   6. Context is fixed at load. Changing the setting reloads the model.
//!
//! PER REQUEST (context is fixed, the request must fit into it)
//!   free             = n_ctx - prompt tokens
//!   answer budget    = min(max_tokens, free)
//!   reasoning budget = off: 0 | low: 512 | medium: 1024 | auto/high/max: up to answer budget - 32
//!                      | N: min(N, answer budget - 32)   (32 tokens always kept for the answer)
//!   Prompt longer than n_ctx - 32 -> drop oldest turns (context shifting), keep system + last message.
//!   Still too long -> error "prompt is longer than context N".
//!
//! COMPUTE RULES
//!   GPU layers                                   -> GPU.
//!   RAM layers, prompt (>= 32 tokens at once)    -> GPU (weights copied over PCIe once per micro-batch).
//!   RAM layers, generation (1 token at a time)   -> CPU (copying weights every token is slower).
//!   Split vs CPU only: decided by the step 4 speed check, never by a model-size rule.
//!   CPU threads: physical cores - 1 (setting).
//!
//! BUFFER RULES (memory kept free, never allocated)
//!   VRAM buffer: 1024 MiB (llama.cpp default). 512 MiB tried as a second candidate,
//!                kept only if it fits more and measures faster.
//!                Weights, KV and compute are measured, so they need no extra padding.
//!                The buffer only covers the desktop / other apps growing after load.
//!   RAM ceiling: 95% of total RAM, other apps included (>= 5% always free).
//!   GB values set by the user replace both.
//!
//! SETTINGS THAT CHANGE THE RESULT
//!   KV cache type        : F16 (Auto) | Q8 (KV x0.53) | Q4 (KV x0.28). Q8/Q4 need flash attention.
//!   Flash attention      : Auto (llama.cpp checks each layer's device).
//!   Batch / micro-batch  : 2048 / 512. Bigger micro-batch = faster prompt + bigger compute buffer.
//!   Parallel chats       : KV x number of chats.
//!   Draft model / MTP / vision projector: extra memory, included in the dry run.
//!   Think mode / reasoning_effort / max_tokens: change the per-request budget, not the placement.
//!
//! WHEN IT CAN STILL CHOKE
//!   Another app takes VRAM after load -> Windows "Shared GPU memory" -> slow.
//!   Another app takes RAM after load  -> OS paging -> slow.
//!   MoE over the RAM ceiling          -> disk reads, first tokens of a new topic slow.
//!   CTX rule 5 B (KV in RAM)          -> slower as the chat grows.
//!   Planned: re-check free VRAM/RAM before each new chat; warn and re-plan when idle.
//!
//! LOG (one block per load)
//!   every candidate with bytes per device, RAM ceiling math, measured speeds, pick + reason,
//!   context llama.cpp actually created.
//!
//! STATUS OF THIS FILE (printed in the log too)
//!   Still estimated here, not measured: layer size, workspace, KV bytes per token, compute buffer.
//!   No speed check yet. The log marks every estimated number with "(est)".
//! 
//! 
//! 
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

use crate::hardware::context_negotiator::ContextResolution;
use crate::hardware::expert_offloading::{detect_moe, MoeModelInfo};
use crate::hardware::governor::HardwareGovernor;
use crate::hardware::schema::optimization::OptimizationControl;
use crate::hardware::schema::profiles::{
    ActiveUserSettings, BusTimings, ComputeCapabilityRates, CpuDirectTiming, CpuHardwareInfo, ExecutionCandidate,
    ExecutionStrategyPlan, GpuExecutionTiming, GpuHardwareInfo, HardwareExecutionComparison,
    HardwareSnapshotInfo, HardwareStateSnapshot, MemoryBreakdown, ModelCapabilities, ModelPlanInfo,
    PcieBusInfo, PcieStreamingTiming, PingPongLatency, RamHardwareInfo, StorageSsdInfo,
    TimingBreakdown, WorkspaceBufferInfo,
};

/// Serializes CUDA / VRAM negotiation across parallel requests.
pub static GLOBAL_HARDWARE_LOCK: Mutex<()> = Mutex::new(());

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

// ─── Core Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineType {
    GGUF,
    ONNX,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InferenceMode {
    Chat,
    Embedding,
    Audio,
    TTS,
}

#[derive(Debug, Clone)]
pub struct ResourceRequest {
    pub engine_type: EngineType,
    pub inference_mode: InferenceMode,
    pub model_size_gb: f64,
    pub model_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementTier {
    GpuOnly,
    Hybrid,
    CpuOnly,
    SsdStreaming,
}

impl std::fmt::Display for PlacementTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlacementTier::GpuOnly => write!(f, "GPU Only (Tier 1)"),
            PlacementTier::Hybrid => write!(f, "Hybrid VRAM+RAM (Tier 2)"),
            PlacementTier::CpuOnly => write!(f, "CPU RAM Only (Tier 3)"),
            PlacementTier::SsdStreaming => write!(f, "SSD Streaming (Tier 4)"),
        }
    }
}

// ─── Decision trace (what the log prints) ────────────────────────────────────

/// One decision step: the facts with numbers, then the verdict.
#[derive(Debug, Clone, Default)]
pub struct TraceStep {
    pub title: String,
    pub lines: Vec<String>,
    pub verdict: String,
}

/// Every input and every decision of one negotiation, in order.
#[derive(Debug, Clone, Default)]
pub struct DecisionTrace {
    pub inputs: Vec<String>,
    pub steps: Vec<TraceStep>,
}

impl DecisionTrace {
    fn input(&mut self, line: String) {
        self.inputs.push(line);
    }
    fn step(&mut self, title: &str, lines: Vec<String>, verdict: String) {
        self.steps.push(TraceStep {
            title: title.to_string(),
            lines,
            verdict,
        });
    }
}

#[derive(Debug, Clone)]
pub struct ResourceGrant {
    pub tier: PlacementTier,
    pub n_gpu_layers: i32,
    pub thread_count: usize,
    pub vram_budget_gb: f64,
    pub ram_budget_gb: f64,
    pub safety_buffer_gb: f64,
    pub expert_cache_budget_gb: f64,
    pub moe_info: Option<MoeModelInfo>,
    pub target_ctx_tokens: usize,
    pub kv_on_gpu: bool,
    pub total_layers: usize,
    pub kv_cache_size_gb: f64,
    pub ssd_bypassed: bool,
    pub placement_state: &'static str,
    /// Step-by-step record of how this grant was decided (printed by the report).
    pub trace: DecisionTrace,
}

// ─── Helpers: Dynamic Math & Geometry ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DynamicRamBudget {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub used_ram_gb: f64,
    pub safety_buffer_gb: f64,
    pub max_allowed_cluaiz_ram: f64,
    pub usable_ram_gb: f64,
}

pub fn calculate_dynamic_ram_headroom(
    opt_control: &OptimizationControl,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> DynamicRamBudget {
    let used_ram_gb = (total_ram_gb - available_ram_gb).max(0.0);
    // Buffer rule: RAM ceiling is 95% of total RAM, other apps included (>= 5% always free).
    let safety_buffer_gb = if let Some(direct_gb) = opt_control.custom_ram_buffer_gb {
        if direct_gb > 0.0 {
            direct_gb.max(0.50).min((total_ram_gb - 2.0).max(1.0))
        } else {
            (total_ram_gb * 0.05).max(0.50)
        }
    } else {
        (total_ram_gb * 0.05).max(0.50)
    };

    let max_allowed_cluaiz_ram = ((total_ram_gb * 0.95) - used_ram_gb).max(0.0);
    let usable_ram_gb = (available_ram_gb - safety_buffer_gb)
        .max(0.0)
        .min(max_allowed_cluaiz_ram);

    DynamicRamBudget {
        total_ram_gb,
        available_ram_gb,
        used_ram_gb,
        safety_buffer_gb,
        max_allowed_cluaiz_ram,
        usable_ram_gb,
    }
}

#[derive(Debug, Clone)]
pub struct LayerGeometry {
    pub total_layers: usize,
    pub layer_size_gb: f64,
    pub dense_backbone_gb: f64,
    pub single_expert_gb: f64,
    pub is_moe: bool,
    /// Where `layer_size_gb` came from (shown in the log).
    pub source: &'static str,
}

pub fn calculate_layer_geometry(
    moe_info: &MoeModelInfo,
    model_gb: f64,
    model_path: Option<&std::path::Path>,
) -> LayerGeometry {
    if moe_info.is_moe {
        let dense_gb = moe_info.dense_backbone_bytes as f64 / GIB;
        let expert_total_gb = moe_info.total_expert_bytes as f64 / GIB;
        let layer_count = moe_info.moe_layer_count.max(1);

        let dense_per_layer = dense_gb / layer_count as f64;
        let expert_per_layer = expert_total_gb / layer_count as f64;
        let layer_size = (dense_per_layer + expert_per_layer).max(0.04);

        let total_experts = (moe_info.expert_count * layer_count) as f64;
        let single_expert_gb = if moe_info.total_expert_bytes > 0 && total_experts > 0.0 {
            (moe_info.total_expert_bytes as f64 / total_experts) / GIB
        } else if moe_info.expert_size_bytes > 0 {
            moe_info.expert_size_bytes as f64 / GIB
        } else {
            0.0
        };

        LayerGeometry {
            total_layers: layer_count,
            layer_size_gb: layer_size,
            dense_backbone_gb: dense_gb,
            single_expert_gb,
            is_moe: true,
            source: "GGUF tensor sizes: (dense + experts) / layers",
        }
    } else {
        let (layer_count, layer_size, base_reserve_gb, source) = if let Some(info) = model_path
            .filter(|p| p.exists() && p.is_file())
            .and_then(|p| crate::metadata::GgufBinaryProber::probe(p).ok())
        {
            let count = info.layer_count.max(1);
            let layer_gb = info.layer_footprint_gb();
            let base_gb = info.base_reserve_bytes as f64 / GIB;
            if layer_gb > 0.001 {
                (count, layer_gb, base_gb, "GGUF tensor sizes of one block")
            } else {
                let gb = ((model_gb - base_gb).max(0.1) / count as f64).max(0.02);
                (count, gb, base_gb, "(file - embeddings) / layers")
            }
        } else {
            let count = 32;
            let layer_gb = (model_gb / count as f64).max(0.02);
            (
                count,
                layer_gb,
                0.0,
                "file size / 32 (GGUF header not readable)",
            )
        };

        LayerGeometry {
            total_layers: layer_count,
            layer_size_gb: layer_size,
            dense_backbone_gb: base_reserve_gb,
            single_expert_gb: 0.0,
            is_moe: false,
            source,
        }
    }
}

/// CPU threads the llama engine will use. Same formula as `interface-engines/llama/src/config.rs`
/// (physical cores - 1, at least 4); the engine prints its final value as `n_threads`.
pub fn engine_threads() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let physical = if cores > 2 { cores / 2 } else { cores };
    if physical > 4 {
        (physical - 1).max(4)
    } else {
        physical
    }
}

/// llama.cpp sends RAM-layer weights to the GPU only for batches of at least this many tokens.
fn op_offload_min_batch() -> String {
    std::env::var("GGML_OP_OFFLOAD_MIN_BATCH").unwrap_or_else(|_| "32".to_string())
}

/// Which blocks llama.cpp puts on the GPU for a given n_gpu_layers.
/// llama.cpp fills from the END: n_gpu_layers = N puts the output head + the last N-1 blocks on GPU
/// (llama-model.cpp: i_gpu_start = n_layer + 1 - n_gpu_layers).
fn gpu_block_ranges(n_gpu_layers: i32, n_layer: usize) -> (String, String) {
    let n_layer_i = n_layer as i32;
    if n_gpu_layers == 0 {
        return (
            "none".into(),
            format!("blk 0-{} + output head", n_layer_i - 1),
        );
    }
    let ngl = if n_gpu_layers < 0 {
        n_layer_i + 1
    } else {
        n_gpu_layers.min(n_layer_i + 1)
    };
    let start = (n_layer_i + 1 - ngl).max(0);
    let gpu = if start <= n_layer_i - 1 {
        format!("blk {}-{} + output head", start, n_layer_i - 1)
    } else {
        "output head only".into()
    };
    let ram = if start > 0 {
        format!("blk 0-{}", start - 1)
    } else {
        "none".into()
    };
    (gpu, ram)
}

fn ctx_setting_str(user_n_ctx: i32) -> String {
    match user_n_ctx {
        -1 => "Max (-1)".into(),
        n if n > 0 => format!("Custom {}", n),
        _ => "Auto (0)".into(),
    }
}

fn gpu_setting_str(n: i32) -> String {
    match n {
        0 => "CPU only (0)".into(),
        n if n > 0 => format!("Custom {} layers", n),
        _ => "Auto (-1)".into(),
    }
}

/// STEP: context + KV, and STEP: who computes. Added at every exit so the log is always complete.
fn push_ctx_and_compute_steps(
    trace: &mut DecisionTrace,
    user_n_ctx: i32,
    ctx: &ContextResolution,
    ctx_why: String,
    kv_on_gpu: bool,
    kv_why: String,
    n_gpu_layers: i32,
    n_layer: usize,
) {
    let kv_kb = ctx.kv_bytes_per_token / 1024.0;
    trace.step(
        "STEP 4  Context + KV cache",
        vec![
            format!("setting    {}", ctx_setting_str(user_n_ctx)),
            format!(
                "result     {} tokens   (model native {})",
                ctx.target_ctx_tokens, ctx.native_max_ctx
            ),
            format!("why        {}", ctx_why),
            format!(
                "KV size    {:.2} GB = {} tokens x {:.0} KB per token (est, GGUF header formula)",
                ctx.required_ctx_gb, ctx.target_ctx_tokens, kv_kb
            ),
            format!(
                "KV where   {}",
                if kv_on_gpu { "GPU VRAM" } else { "System RAM" }
            ),
            format!("why        {}", kv_why),
        ],
        format!(
            "{} tokens, KV {:.2} GB in {}",
            ctx.target_ctx_tokens,
            ctx.required_ctx_gb,
            if kv_on_gpu { "VRAM" } else { "RAM" }
        ),
    );

    let threads = engine_threads();
    let min_batch = op_offload_min_batch();
    let (gpu_blocks, ram_blocks) = gpu_block_ranges(n_gpu_layers, n_layer);
    let mut lines = Vec::new();
    let verdict;
    if n_gpu_layers == 0 {
        lines.push(format!("all blocks     -> CPU, {} threads", threads));
        lines.push("why            GPU not used (user setting 0 or no GPU found)".into());
        verdict = format!("CPU only, {} threads", threads);
    } else if ram_blocks == "none" {
        lines.push(format!("{} -> GPU", gpu_blocks));
        lines.push(
            "embeddings     -> CPU (llama.cpp always keeps the input layer on CPU, tiny work)"
                .into(),
        );
        verdict = "everything on GPU".into();
    } else {
        lines.push(format!("GPU part   {} -> computed on GPU", gpu_blocks));
        lines.push(format!(
            "RAM part   {} + embeddings, computed by:",
            ram_blocks
        ));
        lines.push(format!(
            "             prompt (>= {} tokens at once) -> GPU: weights copied over PCIe once per batch",
            min_batch
        ));
        lines.push(format!(
            "             generation (1 token at a time)  -> CPU, {} threads",
            threads
        ));
        lines.push(
            "why        copying RAM weights to the GPU for EVERY generated token is slower than the CPU;".into(),
        );
        lines.push(format!(
            "           for a prompt, one copy serves {}+ tokens, so the GPU wins there",
            min_batch
        ));
        verdict = format!(
            "GPU: {} | RAM part: prompt on GPU, generation on CPU",
            gpu_blocks
        );
    }
    if n_gpu_layers != 0 && !kv_on_gpu {
        lines.push("attention  ALL blocks -> CPU, because the KV cache is in RAM".into());
        lines.push(
            "           (each GPU block sends its data to the CPU and back for every token)".into(),
        );
    }
    lines.push("speed      not measured: this file picks the placement from size estimates".into());
    trace.step("STEP 5  Who computes", lines, verdict);
}

// ─── Core Negotiation Logic ──────────────────────────────────────────────────

pub fn negotiate_resource(request: &ResourceRequest) -> anyhow::Result<ResourceGrant> {
    negotiate_resource_with_override(request, None)
}

pub fn negotiate_resource_with_override(
    request: &ResourceRequest,
    override_gpu_layers: Option<i32>,
) -> anyhow::Result<ResourceGrant> {
    let (total_vram_gb, live_free_vram_gb) = probe_vram();
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_ram_gb = sys.total_memory() as f64 / GIB;
    let available_ram_gb = sys.available_memory() as f64 / GIB;

    negotiate_resource_with_hardware(
        request,
        override_gpu_layers,
        total_vram_gb,
        live_free_vram_gb,
        total_ram_gb,
        available_ram_gb,
    )
}

pub fn negotiate_resource_with_hardware(
    request: &ResourceRequest,
    override_gpu_layers: Option<i32>,
    total_vram_gb: f64,
    live_free_vram_gb: f64,
    total_ram_gb: f64,
    available_ram_gb: f64,
) -> anyhow::Result<ResourceGrant> {
    let _lock = GLOBAL_HARDWARE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let model_gb = request.model_size_gb;
    let threads = engine_threads();
    let mut trace = DecisionTrace::default();

    // Settings
    let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
    let user_n_gpu_layers = override_gpu_layers.unwrap_or_else(|| match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_gpu_layers
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load().n_gpu_layers
        }
    });
    let user_n_ctx = match request.engine_type {
        EngineType::GGUF => {
            crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load()
                .hardware_and_execution
                .n_ctx
        }
        EngineType::ONNX => {
            crate::hardware::schema::onnx_metadata::OnnxMetadataHeaders::load().n_ctx
        }
    };

    // RAM budget (95% ceiling, other apps included)
    let ram_budget_info =
        calculate_dynamic_ram_headroom(&opt_control, total_ram_gb, available_ram_gb);
    let usable_ram = ram_budget_info.usable_ram_gb;
    let ram_safety = ram_budget_info.safety_buffer_gb;
    let max_allowed_cluaiz_ram = ram_budget_info.max_allowed_cluaiz_ram;

    // VRAM buffer: scaled by GPU size (smaller GPUs need smaller buffers to avoid starving the model)
    let vram_safety = opt_control
        .custom_vram_buffer_gb
        .unwrap_or_else(|| {
            if total_vram_gb <= 4.0 {
                0.50
            }
            // 4 GB cards: 512 MiB
            else if total_vram_gb <= 8.0 {
                0.75
            }
            // 6-8 GB cards: 768 MiB
            else {
                1.00
            } // 10+ GB: 1 GiB
        })
        .max(0.25);

    let moe_info = detect_moe(&request.model_path);
    let geom = calculate_layer_geometry(&moe_info, model_gb, Some(&request.model_path));
    let ggml_workspace_reserve = (0.25 + (model_gb * 0.02)).clamp(0.25, 2.00);

    // ── Inputs ──
    let file_name = request
        .model_path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let gpu_name = HardwareGovernor::load_system_info()
        .ok()
        .and_then(|c| {
            c.silicon_truth
                .accelerators
                .gpus
                .first()
                .map(|g| format!("{} {}", g.vendor, g.model))
        })
        .unwrap_or_else(|| "GPU".into());
    trace.input(format!("MODEL     {}", file_name));
    trace.input(format!(
        "          {:.2} GB | {} blocks | {}",
        model_gb,
        geom.total_layers,
        if moe_info.is_moe {
            format!(
                "MoE: {} experts, {} active per token",
                moe_info.expert_count, moe_info.active_experts_per_token
            )
        } else {
            "dense".into()
        }
    ));
    trace.input(format!(
        "GPU       {} | free {:.2} of {:.2} GB VRAM | buffer kept free {:.2} GB",
        gpu_name.trim(),
        live_free_vram_gb.max(0.0),
        total_vram_gb.max(0.0),
        vram_safety
    ));
    trace.input(format!(
        "RAM       free {:.2} of {:.2} GB | other apps use {:.2} GB",
        available_ram_gb, total_ram_gb, ram_budget_info.used_ram_gb
    ));
    trace.input(format!(
        "          Cluaiz limit = 95% x {:.2} - {:.2} = {:.2} GB",
        total_ram_gb, ram_budget_info.used_ram_gb, max_allowed_cluaiz_ram
    ));
    trace.input(format!(
        "SETTINGS  ctx {} | GPU layers {} | KV {:?} | flash attn {:?} | CPU threads {}",
        ctx_setting_str(user_n_ctx),
        gpu_setting_str(user_n_gpu_layers),
        opt_control.kv_cache_quantization,
        opt_control.flash_attention,
        threads
    ));

    // ── CPU-only path ──
    if user_n_gpu_layers == 0 || total_vram_gb < 0.1 {
        let why_cpu = if user_n_gpu_layers == 0 {
            "user set GPU layers = 0"
        } else {
            "no GPU found"
        };
        trace.step(
            "STEP 1  Full GPU?",
            vec![format!("skipped: {}", why_cpu)],
            "CPU path".into(),
        );

        let ram_needed = model_gb + ggml_workspace_reserve;
        let fits_in_ram = ram_needed <= max_allowed_cluaiz_ram;
        trace.step(
            "STEP 2  Fits in RAM?",
            vec![
                format!(
                    "need       {:.2} GB = weights {:.2} + workspace {:.2} (est)",
                    ram_needed, model_gb, ggml_workspace_reserve
                ),
                format!("allowed    {:.2} GB (Cluaiz limit)", max_allowed_cluaiz_ram),
            ],
            if fits_in_ram {
                "YES -> CPU only".into()
            } else {
                format!(
                    "NO, short by {:.2} GB -> STEP 3",
                    ram_needed - max_allowed_cluaiz_ram
                )
            },
        );

        if !fits_in_ram && !moe_info.is_moe {
            anyhow::bail!(
                "Dense model requires {:.2} GB RAM, which exceeds available RAM ceiling of {:.2} GB (total: {:.2} GB, used: {:.2} GB). Dense models cannot stream from SSD.",
                ram_needed, max_allowed_cluaiz_ram, total_ram_gb, ram_budget_info.used_ram_gb
            );
        }

        if fits_in_ram {
            let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
                &request.model_path,
                user_n_ctx,
                usable_ram,
                total_ram_gb,
                model_gb,
                ram_needed,
            );
            let cache_budget = if moe_info.is_moe {
                moe_info.recommended_cache_budget_gb()
            } else {
                0.0
            };
            push_ctx_and_compute_steps(
                &mut trace,
                user_n_ctx,
                &ctx_resolution,
                format!(
                    "RAM left after weights decides ({})",
                    ctx_resolution.ctx_mode_str
                ),
                false,
                "CPU only: everything is in RAM".into(),
                0,
                geom.total_layers,
            );

            return Ok(ResourceGrant {
                tier: PlacementTier::CpuOnly,
                n_gpu_layers: 0,
                thread_count: threads,
                vram_budget_gb: 0.0,
                ram_budget_gb: usable_ram.max((model_gb * 0.8).min(total_ram_gb - 2.0)),
                safety_buffer_gb: ram_safety,
                expert_cache_budget_gb: cache_budget,
                moe_info: if moe_info.is_moe {
                    Some(moe_info.clone())
                } else {
                    None
                },
                target_ctx_tokens: ctx_resolution.target_ctx_tokens,
                kv_on_gpu: false,
                total_layers: geom.total_layers,
                kv_cache_size_gb: ctx_resolution.required_ctx_gb,
                ssd_bypassed: false,
                placement_state: "State 7: CPU-Only Execution",
                trace,
            });
        } else {
            // MoE larger than the RAM limit on CPU: part of the experts are read from disk.
            let ram_for_experts =
                (max_allowed_cluaiz_ram - geom.dense_backbone_gb - ggml_workspace_reserve).max(0.0);
            let cached_layers = if geom.single_expert_gb > 0.0 {
                let possible_experts = (ram_for_experts / geom.single_expert_gb).floor() as usize;
                (possible_experts / moe_info.expert_count.max(1)).min(geom.total_layers)
            } else {
                ((ram_for_experts / geom.layer_size_gb.max(0.01)).floor() as usize)
                    .min(geom.total_layers)
            };
            let cache_gb = (cached_layers * moe_info.expert_count) as f64 * geom.single_expert_gb;
            trace.step(
                "STEP 3  Bigger than RAM limit",
                vec![
                    "model      MoE -> allowed (dense would be refused here)".into(),
                    format!(
                        "RAM keeps  experts of {} of {} blocks ({:.2} GB)",
                        cached_layers, geom.total_layers, cache_gb
                    ),
                    format!(
                        "disk       experts of {} blocks, read by the OS when used (mmap)",
                        geom.total_layers.saturating_sub(cached_layers)
                    ),
                ],
                "CPU + experts from disk".into(),
            );

            let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
                &request.model_path,
                user_n_ctx,
                usable_ram,
                total_ram_gb,
                model_gb,
                geom.dense_backbone_gb + ggml_workspace_reserve,
            );
            push_ctx_and_compute_steps(
                &mut trace,
                user_n_ctx,
                &ctx_resolution,
                format!(
                    "RAM left after dense weights decides ({})",
                    ctx_resolution.ctx_mode_str
                ),
                false,
                "CPU only: everything is in RAM".into(),
                0,
                geom.total_layers,
            );

            let grant = ResourceGrant {
                tier: PlacementTier::SsdStreaming,
                n_gpu_layers: 0,
                thread_count: threads,
                vram_budget_gb: 0.0,
                ram_budget_gb: (geom.dense_backbone_gb
                    + ggml_workspace_reserve
                    + cache_gb
                    + ctx_resolution.required_ctx_gb)
                    .min(max_allowed_cluaiz_ram),
                safety_buffer_gb: ram_safety,
                expert_cache_budget_gb: cache_gb,
                moe_info: Some(moe_info.clone()),
                target_ctx_tokens: ctx_resolution.target_ctx_tokens,
                kv_on_gpu: false,
                total_layers: geom.total_layers,
                kv_cache_size_gb: ctx_resolution.required_ctx_gb,
                ssd_bypassed: false,
                placement_state: "State 7: CPU-Driven SSD Streaming",
                trace,
            };
            return Ok(grant);
        }
    }

    // ── STEP 1: full GPU? ──
    let vram_free_for_model = (live_free_vram_gb - vram_safety).max(0.0);

    let ctx_resolution_ram = crate::hardware::context_negotiator::resolve_context_window(
        &request.model_path,
        user_n_ctx,
        usable_ram,
        total_ram_gb,
        model_gb,
        0.0,
    );

    // P2: GPU-relevant weight size (subtract embeddings/output that llama.cpp keeps on CPU)
    let gpu_relevant_gb = if geom.is_moe {
        // MoE: only dense/attention goes to GPU; experts handled by tensor_buft_overrides
        geom.dense_backbone_gb
    } else if geom.dense_backbone_gb > 0.01 {
        // Dense: subtract base reserve (embeddings + output head that stay on CPU)
        (model_gb - geom.dense_backbone_gb).max(model_gb * 0.5)
    } else {
        model_gb
    };

    let step1_head = vec![
        format!(
            "VRAM usable {:.2} GB = {:.2} free - {:.2} buffer",
            vram_free_for_model, live_free_vram_gb, vram_safety
        ),
        format!(
            "weights     {:.2} GB (GPU-relevant: {:.2} GB, embeddings {:.2} GB stay on CPU)",
            model_gb,
            gpu_relevant_gb,
            model_gb - gpu_relevant_gb
        ),
        "not counted compute buffer (llama.cpp adds it on top; size not known here)".into(),
    ];

    if gpu_relevant_gb <= vram_free_for_model {
        let vram_for_kv = vram_free_for_model - gpu_relevant_gb;
        let min_kv_gb = (2048.0 * ctx_resolution_ram.kv_bytes_per_token) / GIB;

        if vram_for_kv >= min_kv_gb {
            let max_vram_tokens = if ctx_resolution_ram.kv_bytes_per_token > 0.0 {
                ((vram_for_kv * GIB) / ctx_resolution_ram.kv_bytes_per_token) as usize
            } else {
                ctx_resolution_ram.native_max_ctx
            };
            let target_tokens = if user_n_ctx > 0 {
                user_n_ctx as usize
            } else if user_n_ctx == -1 {
                ctx_resolution_ram.native_max_ctx
            } else {
                max_vram_tokens
                    .min(ctx_resolution_ram.native_max_ctx)
                    .max(4096)
            };
            let actual_kv_gb = (target_tokens as f64 * ctx_resolution_ram.kv_bytes_per_token) / GIB;

            if actual_kv_gb <= vram_for_kv {
                let mut lines = step1_head.clone();
                lines.push(format!("VRAM left   {:.2} GB for KV", vram_for_kv));
                trace.step(
                    "STEP 1  Full GPU?",
                    lines,
                    "YES -> everything on GPU, steps 2-3 skipped".into(),
                );
                let ctx_view = ContextResolution {
                    target_ctx_tokens: target_tokens,
                    ctx_mode_str: ctx_resolution_ram.ctx_mode_str.clone(),
                    required_ctx_gb: actual_kv_gb,
                    native_max_ctx: ctx_resolution_ram.native_max_ctx,
                    kv_bytes_per_token: ctx_resolution_ram.kv_bytes_per_token,
                };
                let ctx_why = match user_n_ctx {
                    n if n > 0 => "user's number, used exactly".to_string(),
                    -1 => "model's native context".to_string(),
                    _ => format!(
                        "largest that fits the {:.2} GB of VRAM left after weights (max {} tokens, min 4096)",
                        vram_for_kv, max_vram_tokens
                    ),
                };
                push_ctx_and_compute_steps(
                    &mut trace,
                    user_n_ctx,
                    &ctx_view,
                    ctx_why,
                    true,
                    format!(
                        "KV {:.2} GB <= VRAM left {:.2} GB",
                        actual_kv_gb, vram_for_kv
                    ),
                    -1,
                    geom.total_layers,
                );
                return Ok(ResourceGrant {
                    tier: PlacementTier::GpuOnly,
                    n_gpu_layers: -1,
                    thread_count: threads,
                    vram_budget_gb: model_gb + actual_kv_gb,
                    ram_budget_gb: 0.0,
                    safety_buffer_gb: vram_safety,
                    expert_cache_budget_gb: 0.0,
                    moe_info: None,
                    target_ctx_tokens: target_tokens,
                    kv_on_gpu: true,
                    total_layers: geom.total_layers,
                    kv_cache_size_gb: actual_kv_gb,
                    ssd_bypassed: false,
                    placement_state:
                        "State 1: Pure VRAM Resident (Weights + KV Cache in Dedicated VRAM)",
                    trace,
                });
            }
        }

        // Weights fit in VRAM, KV does not -> KV in RAM
        let ram_needed = ctx_resolution_ram.required_ctx_gb
            + ggml_workspace_reserve
            + if geom.is_moe {
                moe_info.total_expert_bytes as f64 / GIB
            } else {
                0.0
            };
        if ram_needed <= max_allowed_cluaiz_ram {
            let mut lines = step1_head.clone();
            lines.push(format!(
                "VRAM left   {:.2} GB < KV {:.2} GB",
                vram_for_kv, ctx_resolution_ram.required_ctx_gb
            ));
            trace.step(
                "STEP 1  Full GPU?",
                lines,
                "weights YES, KV NO -> weights on GPU, KV in RAM, steps 2-3 skipped".into(),
            );
            push_ctx_and_compute_steps(
                &mut trace,
                user_n_ctx,
                &ctx_resolution_ram,
                format!("RAM decides ({})", ctx_resolution_ram.ctx_mode_str),
                false,
                format!(
                    "VRAM left {:.2} GB < KV {:.2} GB",
                    vram_for_kv, ctx_resolution_ram.required_ctx_gb
                ),
                -1,
                geom.total_layers,
            );
            return Ok(ResourceGrant {
                tier: PlacementTier::Hybrid,
                n_gpu_layers: -1,
                thread_count: threads,
                vram_budget_gb: model_gb,
                ram_budget_gb: ram_needed,
                safety_buffer_gb: vram_safety,
                expert_cache_budget_gb: 0.0,
                moe_info: None,
                target_ctx_tokens: ctx_resolution_ram.target_ctx_tokens,
                kv_on_gpu: false,
                total_layers: geom.total_layers,
                kv_cache_size_gb: ctx_resolution_ram.required_ctx_gb,
                ssd_bypassed: false,
                placement_state: "State 3: Dedicated VRAM Weights + Host RAM KV Cache",
                trace,
            });
        }
    }

    {
        let mut lines = step1_head.clone();
        if gpu_relevant_gb > vram_free_for_model {
            lines.push(format!(
                "short by    {:.2} GB",
                gpu_relevant_gb - vram_free_for_model
            ));
        }
        trace.step("STEP 1  Full GPU?", lines, "NO -> STEP 2 split".into());
    }

    // ── STEP 2: split GPU + RAM ──
    let vram_for_layers = vram_free_for_model;
    let approx_gpu_layers = if user_n_gpu_layers > 0 {
        user_n_gpu_layers.min(geom.total_layers as i32)
    } else if geom.is_moe && geom.dense_backbone_gb > 0.0 {
        // P1: MoE — count layers by dense/attention size only; experts go to CPU via tensor_buft_overrides
        let dense_per_layer = geom.dense_backbone_gb / geom.total_layers.max(1) as f64;
        if dense_per_layer > 0.0 {
            ((vram_for_layers / dense_per_layer).floor() as i32)
                .min(geom.total_layers as i32)
                .max(0)
        } else {
            0
        }
    } else if geom.layer_size_gb > 0.0 && vram_for_layers > 0.0 {
        // Dense: full layer size
        ((vram_for_layers / geom.layer_size_gb).floor() as i32)
            .min(geom.total_layers as i32)
            .max(0)
    } else {
        0
    };

    let vram_per_layer = if geom.is_moe {
        geom.dense_backbone_gb / geom.total_layers.max(1) as f64
    } else {
        geom.layer_size_gb
    };
    let allocated_vram = (approx_gpu_layers as f64 * vram_per_layer).min(live_free_vram_gb);
    let remaining_layers = (geom.total_layers as i32 - approx_gpu_layers).max(0) as usize;

    let ram_weights_needed = if geom.is_moe {
        // P1: when attention is on GPU, RAM needs: experts of ALL layers (always CPU) + dense of remaining layers
        let all_experts_gb =
            (geom.total_layers * moe_info.expert_count) as f64 * geom.single_expert_gb;
        let remaining_dense_gb =
            remaining_layers as f64 * (geom.dense_backbone_gb / geom.total_layers.max(1) as f64);
        all_experts_gb + remaining_dense_gb
    } else {
        remaining_layers as f64 * geom.layer_size_gb + geom.dense_backbone_gb
    };

    let total_cluaiz_weights_and_workspace = ram_weights_needed + ggml_workspace_reserve;
    let fits_under_ceiling = total_cluaiz_weights_and_workspace <= max_allowed_cluaiz_ram;

    {
        let (gpu_blocks, ram_blocks) = gpu_block_ranges(approx_gpu_layers, geom.total_layers);
        let gpu_count_line = if user_n_gpu_layers > 0 {
            format!("GPU layers  {} (user setting)", approx_gpu_layers)
        } else {
            format!(
                "GPU layers  {} = floor({:.2} GB usable / {:.2} GB per block)",
                approx_gpu_layers, vram_for_layers, geom.layer_size_gb
            )
        };
        let mut lines = vec![
            format!(
                "block size  {:.2} GB (est: {})",
                geom.layer_size_gb, geom.source
            ),
            gpu_count_line,
            format!(
                "            llama.cpp fills from the end: {} on GPU, {:.2} GB (est)",
                gpu_blocks, allocated_vram
            ),
            "            (n_gpu_layers counts the output head as one layer)".into(),
            format!("RAM blocks  {} ({} blocks)", ram_blocks, remaining_layers),
        ];
        if geom.is_moe {
            lines.push(format!(
                "RAM weights {:.2} GB = shared/dense {:.2} + experts of {} blocks {:.2}",
                ram_weights_needed,
                geom.dense_backbone_gb,
                remaining_layers,
                ram_weights_needed - geom.dense_backbone_gb
            ));
            lines.push(
                "MoE note    whole blocks are split; experts are NOT moved separately".into(),
            );
        } else {
            lines.push(format!(
                "RAM weights {:.2} GB = {} blocks x {:.2} + embeddings {:.2}",
                ram_weights_needed, remaining_layers, geom.layer_size_gb, geom.dense_backbone_gb
            ));
        }
        lines.push(format!(
            "RAM need    {:.2} GB = weights {:.2} + workspace {:.2} (est)  [KV added in step 4]",
            total_cluaiz_weights_and_workspace, ram_weights_needed, ggml_workspace_reserve
        ));
        lines.push(format!(
            "RAM allowed {:.2} GB (Cluaiz limit)",
            max_allowed_cluaiz_ram
        ));
        let verdict = if fits_under_ceiling {
            "FITS -> GPU + RAM split, step 3 skipped".to_string()
        } else {
            format!(
                "DOES NOT FIT, over by {:.2} GB -> STEP 3",
                total_cluaiz_weights_and_workspace - max_allowed_cluaiz_ram
            )
        };
        trace.step("STEP 2  Split GPU + RAM", lines, verdict);
    }

    if !fits_under_ceiling && !geom.is_moe {
        anyhow::bail!(
            "Dense model with {} CPU layers requires {:.2} GB RAM, exceeding available RAM ceiling of {:.2} GB (total: {:.2} GB, used: {:.2} GB). Dense models cannot stream from SSD.",
            remaining_layers, total_cluaiz_weights_and_workspace, max_allowed_cluaiz_ram, total_ram_gb, ram_budget_info.used_ram_gb
        );
    }

    let (tier, cached_layers, overflow_layers, actual_cache_gb) = if fits_under_ceiling {
        let cache_gb = if geom.is_moe {
            (remaining_layers * moe_info.expert_count) as f64 * geom.single_expert_gb
        } else {
            0.0
        };
        (PlacementTier::Hybrid, remaining_layers, 0usize, cache_gb)
    } else {
        let ram_for_experts =
            (max_allowed_cluaiz_ram - geom.dense_backbone_gb - ggml_workspace_reserve).max(0.0);
        let cached = if geom.is_moe && geom.single_expert_gb > 0.0 {
            let possible_experts = (ram_for_experts / geom.single_expert_gb).floor() as usize;
            (possible_experts / moe_info.expert_count.max(1)).min(remaining_layers)
        } else {
            ((ram_for_experts / geom.layer_size_gb.max(0.01)).floor() as usize)
                .min(remaining_layers)
        };
        let overflow = remaining_layers.saturating_sub(cached);
        let cache_gb = if geom.is_moe {
            (cached * moe_info.expert_count) as f64 * geom.single_expert_gb
        } else {
            0.0
        };
        (PlacementTier::SsdStreaming, cached, overflow, cache_gb)
    };

    if tier == PlacementTier::SsdStreaming {
        trace.step(
            "STEP 3  Bigger than VRAM + RAM limit",
            vec![
                "model       MoE -> allowed (dense would be refused here)".into(),
                format!("RAM keeps   experts of {} blocks ({:.2} GB)", cached_layers, actual_cache_gb),
                format!(
                    "disk        experts of {} blocks: the OS reads them when used (mmap), not all at once",
                    overflow_layers
                ),
            ],
            "GPU + RAM + experts read from disk".into(),
        );
    }

    let resident_weights_gb = if tier == PlacementTier::SsdStreaming {
        geom.dense_backbone_gb + actual_cache_gb
    } else {
        ram_weights_needed
    };
    let total_ram_footprint = resident_weights_gb + ggml_workspace_reserve;

    let ctx_resolution = crate::hardware::context_negotiator::resolve_context_window(
        &request.model_path,
        user_n_ctx,
        usable_ram,
        total_ram_gb,
        model_gb,
        total_ram_footprint,
    );

    let leftover_vram = (live_free_vram_gb - allocated_vram - vram_safety).max(0.0);
    let kv_on_gpu = leftover_vram >= ctx_resolution.required_ctx_gb;
    let kv_why = if kv_on_gpu {
        format!(
            "KV {:.2} GB <= VRAM left after layers {:.2} GB (locked on GPU)",
            ctx_resolution.required_ctx_gb, leftover_vram
        )
    } else {
        format!(
            "VRAM left after layers {:.2} GB < KV {:.2} GB (fallback to Host RAM)",
            leftover_vram, ctx_resolution.required_ctx_gb
        )
    };
    push_ctx_and_compute_steps(
        &mut trace,
        user_n_ctx,
        &ctx_resolution,
        format!(
            "RAM left after weights decides ({})",
            ctx_resolution.ctx_mode_str
        ),
        kv_on_gpu,
        kv_why,
        approx_gpu_layers,
        geom.total_layers,
    );

    let final_ram_budget =
        (total_ram_footprint + ctx_resolution.required_ctx_gb).min(max_allowed_cluaiz_ram);

    let grant = ResourceGrant {
        tier,
        n_gpu_layers: approx_gpu_layers,
        thread_count: threads,
        vram_budget_gb: allocated_vram,
        ram_budget_gb: final_ram_budget,
        safety_buffer_gb: ram_safety,
        expert_cache_budget_gb: actual_cache_gb,
        moe_info: if moe_info.is_moe {
            Some(moe_info.clone())
        } else {
            None
        },
        target_ctx_tokens: ctx_resolution.target_ctx_tokens,
        kv_on_gpu,
        total_layers: geom.total_layers,
        kv_cache_size_gb: ctx_resolution.required_ctx_gb,
        ssd_bypassed: false,
        placement_state: if tier == PlacementTier::Hybrid {
            "State 4: Memory-Resident Hybrid (Zero SSD Swap)"
        } else {
            "State 6: True SSD Streaming Fallback (Active Page-Cache Streaming)"
        },
        trace,
    };

    Ok(grant)
}

/// Kept for callers; does nothing (Windows does not need a working-set quota here).
#[cfg(windows)]
pub fn apply_windows_hard_memory_quota(_usable_ram_gb: f64) {}

#[cfg(not(windows))]
pub fn apply_windows_hard_memory_quota(_usable_ram_gb: f64) {}

/// Total VRAM of all GPUs and live free VRAM of GPU 0 (NVML; falls back to system probe).
pub fn probe_vram() -> (f64, f64) {
    if let Ok(nvml) = nvml_wrapper::Nvml::init() {
        if let Ok(dev) = nvml.device_by_index(0) {
            if let Ok(m) = dev.memory_info() {
                let total = m.total as f64 / GIB;
                let free = m.free as f64 / GIB;
                if total > 0.0 {
                    return (total, free);
                }
            }
        }
    }
    let control = HardwareGovernor::load_system_info().unwrap_or_else(|_| crate::hardware::system_info::HardwareOrchestrator::probe_live());
    let total_vram_gb: f64 = control
        .silicon_truth
        .accelerators
        .gpus
        .iter()
        .map(|g| g.vram_total_gb)
        .sum();

    let free_vram_gb: f64 = control
        .silicon_truth
        .accelerators
        .gpus
        .iter()
        .map(|g| g.vram_available_gb)
        .sum();

    (total_vram_gb, if free_vram_gb > 0.0 { free_vram_gb } else { total_vram_gb * 0.70 })
}

/// Prints the decision step by step: inputs, every step with its numbers and verdict, and the result.
pub fn print_negotiator_full_report(title: &str, req: &ResourceRequest, grant: &ResourceGrant) {
    let bar = "=".repeat(92);
    let thin = "-".repeat(92);
    let mut out = String::new();
    out.push_str(&format!(
        "\n{}\n  CLUAIZ PLACEMENT  |  {}\n{}\n",
        bar, title, bar
    ));

    for line in &grant.trace.inputs {
        out.push_str(&format!("  {}\n", line));
    }

    for step in &grant.trace.steps {
        out.push_str(&format!("{}\n  {}\n", thin, step.title));
        for line in &step.lines {
            out.push_str(&format!("      {}\n", line));
        }
        out.push_str(&format!("   => {}\n", step.verdict));
    }

    let (gpu_blocks, ram_blocks) = gpu_block_ranges(grant.n_gpu_layers, grant.total_layers);
    out.push_str(&format!("{}\n  RESULT\n", bar));
    out.push_str(&format!(
        "      GPU     {} (VRAM budget {:.2} GB)\n",
        gpu_blocks, grant.vram_budget_gb
    ));
    out.push_str(&format!(
        "      RAM     {} (budget {:.2} GB incl. KV)\n",
        ram_blocks, grant.ram_budget_gb
    ));
    out.push_str(&format!(
        "      context {} tokens, KV {:.2} GB in {}\n",
        grant.target_ctx_tokens,
        grant.kv_cache_size_gb,
        if grant.kv_on_gpu { "VRAM" } else { "RAM" }
    ));
    out.push_str(&format!(
        "      tier    {} | {}\n",
        grant.tier, grant.placement_state
    ));
    out.push_str(&format!("      model   {:?}\n", req.model_path));
    out.push_str(&format!("{}\n", bar));
    eprintln!("{}", out);
}

// ─── Candidate Execution Planner (Single Source of Truth) ───────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionPlanRequest {
    pub model: Option<String>,
    pub weights_gb: Option<f64>,
    pub model_path: Option<PathBuf>,
    pub batch_tokens: Option<usize>,
    pub total_layers: Option<usize>,
    pub n_ctx: Option<usize>,
    pub vram_buffer_gb: Option<f64>,
    pub ram_buffer_gb: Option<f64>,
}

/// Evaluates real hardware capability and generates the 5 llama.cpp execution candidates
/// (A: Full GPU, B: Weights GPU KV RAM, C: Split Hybrid, D: CPU Only, E: MoE Split)
/// picking the winner based on highest generation TPS without false PCIe streaming penalties.
pub fn plan_execution_candidates(req: &ExecutionPlanRequest) -> Result<HardwareExecutionComparison, String> {
    let loaded_ctrl = HardwareGovernor::load_system_info().unwrap_or_default();
    let control = if loaded_ctrl.silicon_truth.accelerators.gpus.is_empty() {
        crate::hardware::system_info::HardwareOrchestrator::probe_live()
    } else {
        loaded_ctrl
    };

    // 1. Live System Hardware Probe
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let total_ram_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let used_ram_gb = sys.used_memory() as f64 / 1_073_741_824.0;
    let free_ram_gb = sys.available_memory() as f64 / 1_073_741_824.0;
    let cpu_util_pct = sys.global_cpu_info().cpu_usage();

    let cpu_brand = if !control.silicon_truth.cpu.brand.is_empty() {
        control.silicon_truth.cpu.brand.clone()
    } else {
        sys.cpus()
            .first()
            .map(|c| c.brand().to_string())
            .unwrap_or_else(|| "Host CPU".into())
    };
    let cpu_cores = if control.silicon_truth.cpu.physical_cores > 0 {
        control.silicon_truth.cpu.physical_cores as usize
    } else {
        sys.physical_core_count().unwrap_or(8)
    };
    let cpu_threads = if control.silicon_truth.cpu.logical_threads > 0 {
        control.silicon_truth.cpu.logical_threads as usize
    } else {
        sys.cpus().len().max(16)
    };
    let cpu_clock_ghz = if control.silicon_truth.cpu.boost_clock_mhz > 0.0 {
        control.silicon_truth.cpu.boost_clock_mhz / 1000.0
    } else {
        sys.cpus().first().map(|c| c.frequency() as f64 / 1000.0).unwrap_or(3.2)
    };
    let cpu_peak_gflops = if control.silicon_truth.cpu.gflops_fp32 > 0.0 {
        control.silicon_truth.cpu.gflops_fp32
    } else {
        (cpu_cores as f64) * cpu_clock_ghz * 32.0 // AVX2 SIMD: 32 FLOPs/cycle
    };
    let ram_read_bw = control.silicon_truth.memory.bandwidth_gbps.max(38.4);
    let ram_latency_ns = if control.silicon_truth.memory.memory_latency_ns > 0.0 {
        control.silicon_truth.memory.memory_latency_ns
    } else {
        75.0
    };

    // 2. Real Accelerator Probe
    let (probed_total_vram, live_free_vram) = probe_vram();
    let maybe_gpu = control.silicon_truth.accelerators.gpus.first();
    let has_gpu = probed_total_vram > 0.0 || maybe_gpu.is_some();

    let gpu_model = maybe_gpu
        .map(|g| g.model.clone())
        .unwrap_or_else(|| if has_gpu { "Dedicated Accelerator".to_string() } else { "None".to_string() });
    let gpu_total_vram_gb = if let Some(g) = maybe_gpu {
        if g.vram_total_gb > 0.0 { g.vram_total_gb } else { probed_total_vram }
    } else {
        probed_total_vram
    };
    let gpu_free_vram_gb = if live_free_vram > 0.0 {
        live_free_vram
    } else if let Some(g) = maybe_gpu {
        if g.vram_available_gb > 0.0 { g.vram_available_gb } else { gpu_total_vram_gb * 0.70 }
    } else {
        gpu_total_vram_gb * 0.70
    };
    let gpu_used_vram_gb = (gpu_total_vram_gb - gpu_free_vram_gb).max(0.0);
    let gpu_vram_bw = maybe_gpu.map(|g| g.bandwidth_gbps).unwrap_or(192.0).max(128.0);
    let gpu_pcie_bw = maybe_gpu.map(|g| g.pcie_h2d_bandwidth_gbps).unwrap_or(7.8).max(4.0);
    let gpu_pcie_roundtrip_us = maybe_gpu.map(|g| g.pcie_roundtrip_latency_us).unwrap_or(12.0).max(5.0);
    let gpu_peak_tflops = maybe_gpu.map(|g| g.tflops_fp32).unwrap_or(9.1).max(2.0);

    // 3. User Configurations from .cluaiz/engine/config/ (Single Source of Truth)
    let env = crate::environment::EnvironmentManager::current();
    let opt_control = HardwareGovernor::load_optimization_settings().unwrap_or_default();
    let gguf_config = crate::hardware::schema::gguf_metadata::GgufMetadataHeaders::load();

    let kv_element_bytes = match opt_control.kv_cache_quantization {
        crate::hardware::schema::optimization::KvCacheQuantization::Kv4 => 0.5f64,
        crate::hardware::schema::optimization::KvCacheQuantization::Kv8 => 1.0f64,
        _ => 2.0f64, // Kv16 or Auto default (FP16)
    };

    let vram_safety_buffer_gb = req.vram_buffer_gb
        .or(opt_control.custom_vram_buffer_gb)
        .unwrap_or_else(|| {
            // If GPU total VRAM is small (<= 4GB), use 0.25 GB (256 MB) buffer so small models fit
            if gpu_total_vram_gb <= 4.1 {
                0.25
            } else {
                0.50
            }
        });
    let ram_safety_buffer_gb = req.ram_buffer_gb
        .or(opt_control.custom_ram_buffer_gb)
        .unwrap_or_else(|| (total_ram_gb * 0.05).max(1.0));
    let gpu_usable_after_buffer_gb = (gpu_free_vram_gb - vram_safety_buffer_gb).max(0.0);
    let ram_usable_after_buffer_gb = (free_ram_gb - ram_safety_buffer_gb).max(0.0);

    // 4. Model Identification & Autonomous Geometry Resolution via model_registry.json & GgufBinaryProber
    let batch_tokens = req.batch_tokens
        .unwrap_or(gguf_config.hardware_and_execution.batch_size)
        .max(1);

    let reg_path = env.model_registry_json_path();
    let registry_data: Option<serde_json::Value> = if reg_path.exists() {
        std::fs::read_to_string(&reg_path).ok().and_then(|c| serde_json::from_str(&c).ok())
    } else {
        None
    };

    let requested_id = req.model.clone()
        .or_else(|| req.model_path.as_ref().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string())));

    // Look up entry in model_registry.json installed_models
    let matched_entry = registry_data.as_ref().and_then(|reg| {
        let installed = reg.get("installed_models")?.as_object()?;
        if let Some(ref target) = requested_id {
            if target == "auto" {
                return installed.iter().next().map(|(k, v)| (k.clone(), v.clone()));
            }
            // 1. Exact match
            if let Some(entry) = installed.get(target) {
                return Some((target.clone(), entry.clone()));
            }
            // 2. Normalized match (exact match after normalization, no .contains() guesswork)
            let clean_req = target.to_lowercase().replace([':', '_', '.'], "-");
            for (k, v) in installed {
                let clean_k = k.to_lowercase().replace([':', '_', '.'], "-");
                if clean_k == clean_req {
                    return Some((k.clone(), v.clone()));
                }
            }
            None
        } else {
            // Default to first installed model only if no model ID, path or weights are provided
            if req.weights_gb.is_none() && req.model_path.is_none() {
                installed.iter().next().map(|(k, v)| (k.clone(), v.clone()))
            } else {
                None
            }
        }
    });

    // Extract dynamic metadata from registry entry if present
    let (reg_id, reg_file_path, reg_size_bytes, reg_arch, reg_context, reg_tasks, reg_category) = if let Some((id, ref entry)) = matched_entry {
        let local_dir = entry.get("local_dir").and_then(|v| v.as_str()).unwrap_or("");
        let files = entry.get("files").and_then(|v| v.as_array());
        let primary_file = files.and_then(|arr| {
            arr.iter().find(|f| f.get("is_primary").and_then(|p| p.as_bool()).unwrap_or(false))
                .or_else(|| arr.first())
        });
        let primary_name = primary_file.and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
        let primary_size = primary_file.and_then(|f| f.get("size_bytes")).and_then(|s| s.as_u64()).unwrap_or(0);
        let file_path = if !local_dir.is_empty() && !primary_name.is_empty() {
            Some(PathBuf::from(local_dir).join(primary_name))
        } else {
            None
        };

        let meta = entry.get("metadata");
        let arch = meta.and_then(|m| m.get("architecture")).and_then(|a| a.as_str()).unwrap_or("").to_string();
        let ctx_str = meta.and_then(|m| m.get("context_window")).and_then(|c| c.as_str()).unwrap_or("");
        let clean_digits: String = ctx_str.chars().filter(|c| c.is_ascii_digit()).collect();
        let parsed_ctx = if ctx_str.to_lowercase().contains('k') {
            clean_digits.parse::<usize>().unwrap_or(0) * 1024
        } else {
            clean_digits.parse::<usize>().unwrap_or(0)
        };

        let tasks: Vec<String> = entry.get("supported_tasks")
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();
        let cat = entry.get("category").and_then(|c| c.as_str()).unwrap_or("").to_string();

        (Some(id), file_path, primary_size, arch, parsed_ctx, tasks, cat)
    } else {
        (None, None, 0u64, String::new(), 0, Vec::new(), String::new())
    };

    let resolved_path = req.model_path.clone().or(reg_file_path);

    // 🛡️ Zero Fallback Enforcement: If a specific model ID was requested but not found in registry or disk, return clean error
    if let Some(ref target) = requested_id {
        if reg_id.is_none() && (resolved_path.is_none() || !resolved_path.as_ref().unwrap().exists()) {
            return Err(format!("Model '{}' not found in registry and no valid file exists on disk", target));
        }
    }

    // Dynamic extraction from real GGUF binary headers if file exists on disk
    let (resolved_model_name, weights_gb, total_layers, n_ctx, architecture, is_moe, kv_cache_gb, compute_buffer_gb, head_count, head_count_kv, head_dim) = if let Some(ref path) = resolved_path {
        if path.exists() && path.is_file() {
            let m_name = reg_id.unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "custom-model".into()));
            if let Ok(arch) = crate::metadata::gguf_prober::GgufBinaryProber::probe(path) {
                let w_gb = arch.file_size_bytes as f64 / GIB;
                let layers = if arch.layer_count > 0 { arch.layer_count } else { req.total_layers.unwrap_or(32) };
                let config_ctx = if gguf_config.hardware_and_execution.n_ctx > 0 { gguf_config.hardware_and_execution.n_ctx as usize } else { 0 };
                let user_req_ctx = req.n_ctx.unwrap_or(config_ctx);
                let native_max_ctx = if arch.context_length > 0 { arch.context_length } else if reg_context > 0 { reg_context } else { 4096 };
                let ctx = if user_req_ctx > 0 {
                    user_req_ctx
                } else {
                    4096.min(native_max_ctx).max(2048)
                };
                let arch_name = if !arch.architecture.is_empty() { arch.architecture.clone() } else if !reg_arch.is_empty() { reg_arch } else { "transformer".to_string() };
                let kv_bytes = arch.kv_bytes_per_token(kv_element_bytes); // Scaled by user's kv_cache_quantization setting
                let kv_gb = (ctx as f64 * kv_bytes) / GIB;
                let comp_gb = ((batch_tokens as f64) * 0.002).max(0.15);
                (m_name, w_gb, layers, ctx, arch_name, arch.is_moe, kv_gb, comp_gb, arch.head_count, arch.head_count_kv, arch.head_dim)
            } else {
                let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(reg_size_bytes);
                let w_gb = (file_bytes as f64 / GIB).max(0.01);
                let layers = req.total_layers.unwrap_or(32);
                let user_req_ctx = req.n_ctx.unwrap_or(0);
                let native_max_ctx = if reg_context > 0 { reg_context } else { 4096 };
                let ctx = if user_req_ctx > 0 { user_req_ctx } else { 4096.min(native_max_ctx).max(2048) };
                let arch_name = if !reg_arch.is_empty() { reg_arch } else { "transformer".to_string() };
                let kv_bytes = 2.0 * layers as f64 * 8.0 * 128.0 * kv_element_bytes;
                let kv_gb = (ctx as f64 * kv_bytes) / GIB;
                let comp_gb = ((batch_tokens as f64) * 0.002).max(0.08);
                (m_name, w_gb, layers, ctx, arch_name, false, kv_gb, comp_gb, 16, 8, 128)
            }
        } else if reg_size_bytes > 0 {
            let m_name = reg_id.unwrap_or_else(|| "custom-model".into());
            let w_gb = (reg_size_bytes as f64 / GIB).max(0.01);
            let layers = req.total_layers.unwrap_or(32);
            let user_req_ctx = req.n_ctx.unwrap_or(0);
            let native_max_ctx = if reg_context > 0 { reg_context } else { 4096 };
            let ctx = if user_req_ctx > 0 { user_req_ctx } else { 4096.min(native_max_ctx).max(2048) };
            let arch_name = if !reg_arch.is_empty() { reg_arch } else { "transformer".to_string() };
            let kv_bytes = 2.0 * layers as f64 * 8.0 * 128.0 * kv_element_bytes;
            let kv_gb = (ctx as f64 * kv_bytes) / GIB;
            let comp_gb = ((batch_tokens as f64) * 0.002).max(0.08);
            (m_name, w_gb, layers, ctx, arch_name, false, kv_gb, comp_gb, 16, 8, 128)
        } else if let Some(w_gb) = req.weights_gb {
            let m_name = reg_id.or(requested_id.clone()).unwrap_or_else(|| "custom-model".into());
            let layers = req.total_layers.unwrap_or_else(|| {
                if w_gb <= 1.5 { 16 } else if w_gb <= 3.0 { 24 } else { 32 }
            });
            let ctx = req.n_ctx.unwrap_or(4096);
            let arch_name = if !reg_arch.is_empty() { reg_arch } else { "transformer".to_string() };
            let kv_heads = if w_gb <= 3.0 { 4.0 } else { 8.0 };
            let kv_bytes = 2.0 * layers as f64 * kv_heads * 128.0 * kv_element_bytes;
            let kv_gb = (ctx as f64 * kv_bytes) / GIB;
            let comp_gb = ((batch_tokens as f64) * 0.002).max(0.08);
            (m_name, w_gb, layers, ctx, arch_name, false, kv_gb, comp_gb, 16, 8, 128)
        } else {
            return Err("Model file not found and no weights_gb provided for execution planning".to_string());
        }
    } else if let Some(w_gb) = req.weights_gb {
        let m_name = reg_id.or(requested_id.clone()).unwrap_or_else(|| "custom-model".into());
        let layers = req.total_layers.unwrap_or_else(|| {
            if w_gb <= 1.5 { 16 } else if w_gb <= 3.0 { 24 } else { 32 }
        });
        let ctx = req.n_ctx.unwrap_or(4096);
        let arch_name = if !reg_arch.is_empty() { reg_arch } else { "transformer".to_string() };
        let kv_heads = if w_gb <= 3.0 { 4.0 } else { 8.0 };
        let kv_bytes = 2.0 * layers as f64 * kv_heads * 128.0 * kv_element_bytes;
        let kv_gb = (ctx as f64 * kv_bytes) / GIB;
        let comp_gb = ((batch_tokens as f64) * 0.002).max(0.08);
        (m_name, w_gb, layers, ctx, arch_name, false, kv_gb, comp_gb, 16, 8, 128)
    } else {
        return Err("No model specified and registry has no installed models".to_string());
    };
    let layer_size_gb = weights_gb / (total_layers.max(1) as f64);

    // 5. Hardware Timing Foundations
    // Active parameters (Q4 quantization ~0.55 bytes/param)
    let active_params = (weights_gb * 1_000_000_000.0) / 0.55;
    let flops_per_token = 2.0 * active_params;

    // Generation latency per token (Memory bandwidth bound)
    let vram_read_ms = (weights_gb / (gpu_vram_bw * 0.70)) * 1000.0;
    let cuda_compute_ms = (flops_per_token / (gpu_peak_tflops * 1_000_000_000_000.0 * 0.60)) * 1000.0;
    let gpu_gen_ms = f64::max(vram_read_ms, cuda_compute_ms) + 0.5;

    let ram_read_ms = (weights_gb / (ram_read_bw * 0.70)) * 1000.0;
    let cpu_simd_ms = (flops_per_token / (cpu_peak_gflops * 1_000_000_000.0 * 0.50)) * 1000.0;
    let cpu_gen_ms = ram_read_ms + cpu_simd_ms + 1.0;

    // Prefill TTFT calculation (512 tokens prompt evaluation)
    let prefill_tokens = 512.0f64;
    let prefill_flops = 2.0 * prefill_tokens * active_params;
    let gpu_prefill_compute_ms = (prefill_flops / (gpu_peak_tflops * 1_000_000_000_000.0 * 0.60)) * 1000.0;
    let gpu_prefill_total_ms = (gpu_prefill_compute_ms + vram_read_ms * 0.50 + 2.0).max(5.0);
    let cpu_prefill_compute_ms = (prefill_flops / (cpu_peak_gflops * 1_000_000_000.0 * 0.50)) * 1000.0;
    let cpu_prefill_total_ms = (cpu_prefill_compute_ms + ram_read_ms + 5.0).max(15.0);

    // PCIe Streaming baseline (transferring weights over PCIe on every token)
    let pcie_transfer_ms = (weights_gb / gpu_pcie_bw) * 1000.0;
    let bus_hop_penalty_ms = (gpu_pcie_roundtrip_us / 1000.0) * 2.0;
    let pcie_streaming_ms = pcie_transfer_ms + vram_read_ms + cuda_compute_ms + bus_hop_penalty_ms;

    let timing_breakdown = TimingBreakdown {
        vram_execution: if has_gpu {
            Some(GpuExecutionTiming {
                vram_read_ms: (vram_read_ms * 100.0).round() / 100.0,
                cuda_compute_ms: (cuda_compute_ms * 100.0).round() / 100.0,
                total_ms: (gpu_gen_ms * 100.0).round() / 100.0,
            })
        } else {
            None
        },
        pcie_streaming: if has_gpu {
            Some(PcieStreamingTiming {
                pcie_transfer_ms: (pcie_transfer_ms * 100.0).round() / 100.0,
                vram_read_ms: (vram_read_ms * 100.0).round() / 100.0,
                cuda_compute_ms: (cuda_compute_ms * 100.0).round() / 100.0,
                bus_hop_penalty_ms: (bus_hop_penalty_ms * 100.0).round() / 100.0,
                total_ms: (pcie_streaming_ms * 100.0).round() / 100.0,
            })
        } else {
            None
        },
        cpu_direct: CpuDirectTiming {
            ram_read_ms: (ram_read_ms * 100.0).round() / 100.0,
            simd_compute_ms: (cpu_simd_ms * 100.0).round() / 100.0,
            total_ms: (cpu_gen_ms * 100.0).round() / 100.0,
        },
    };

    let bus_ping_pong_latency = PingPongLatency {
        ram_to_cpu_latency_ns: ram_latency_ns,
        ram_to_gpu_pcie_roundtrip_us: gpu_pcie_roundtrip_us,
    };

    let hardware_state = HardwareStateSnapshot {
        cpu_brand: cpu_brand.clone(),
        cpu_cores,
        cpu_threads,
        cpu_utilization_pct: cpu_util_pct,
        cpu_clock_ghz: (cpu_clock_ghz * 100.0).round() / 100.0,
        cpu_peak_gflops: (cpu_peak_gflops * 10.0).round() / 10.0,
        ram_total_gb: (total_ram_gb * 100.0).round() / 100.0,
        ram_used_gb: (used_ram_gb * 100.0).round() / 100.0,
        ram_free_gb: (free_ram_gb * 100.0).round() / 100.0,
        ram_usable_after_buffer_gb: (ram_usable_after_buffer_gb * 100.0).round() / 100.0,
        ram_read_bandwidth_gbps: (ram_read_bw * 10.0).round() / 10.0,
        ram_latency_ns: (ram_latency_ns * 10.0).round() / 10.0,
        gpu_model: gpu_model.clone(),
        gpu_total_vram_gb: (gpu_total_vram_gb * 100.0).round() / 100.0,
        gpu_used_vram_gb: (gpu_used_vram_gb * 100.0).round() / 100.0,
        gpu_free_vram_gb: (gpu_free_vram_gb * 100.0).round() / 100.0,
        gpu_usable_after_buffer_gb: (gpu_usable_after_buffer_gb * 100.0).round() / 100.0,
        gpu_vram_bandwidth_gbps: (gpu_vram_bw * 10.0).round() / 10.0,
        gpu_pcie_bandwidth_gbps: (gpu_pcie_bw * 10.0).round() / 10.0,
        gpu_pcie_roundtrip_us: (gpu_pcie_roundtrip_us * 10.0).round() / 10.0,
        gpu_peak_tflops: (gpu_peak_tflops * 10.0).round() / 10.0,
    };

    // 6. Evaluate The 5 Execution Candidates
    let mut candidates = Vec::with_capacity(5);

    // Candidate A: Full GPU (VRAM-Resident)
    let a_required_vram = weights_gb + kv_cache_gb + compute_buffer_gb;
    let a_feasible = has_gpu && (a_required_vram <= gpu_usable_after_buffer_gb);
    let a_tps = if gpu_gen_ms > 0.0 { (1000.0 / gpu_gen_ms * 10.0).round() / 10.0 } else { 0.0 };
    candidates.push(ExecutionCandidate {
        id: "candidate_a_full_gpu".to_string(),
        name: "Candidate A: Full GPU (VRAM-Resident)".to_string(),
        feasible: a_feasible,
        ineligibility_reason: if a_feasible {
            None
        } else if !has_gpu {
            Some("No CUDA GPU detected".into())
        } else {
            Some(format!(
                "Required VRAM {:.2} GB (weights {:.2} + KV {:.2} + workspace {:.2}) exceeds usable VRAM {:.2} GB",
                a_required_vram, weights_gb, kv_cache_gb, compute_buffer_gb, gpu_usable_after_buffer_gb
            ))
        },
        gpu_layers: if a_feasible { total_layers } else { 0 },
        cpu_layers: 0,
        total_layers,
        vram_required_gb: (a_required_vram * 100.0).round() / 100.0,
        ram_required_gb: 0.10, // Minimal host runtime footprint
        kv_location: "VRAM".into(),
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        prompt_ms_per_token: ((vram_read_ms / batch_tokens as f64 + cuda_compute_ms) * 100.0).round() / 100.0,
        generation_ms_per_token: (gpu_gen_ms * 100.0).round() / 100.0,
        generation_tps: if a_feasible { a_tps } else { 0.0 },
        ttft_ms: if a_feasible { (gpu_prefill_total_ms * 10.0).round() / 10.0 } else { 0.0 },
        is_winner: false,
        notes: "Zero PCIe memory transfer during token generation. Maximum throughput.".into(),
    });

    // Candidate B: Weights GPU, KV in Host RAM (offload_kqv=false)
    let b_vram_needed = weights_gb + compute_buffer_gb;
    let b_feasible = has_gpu && (b_vram_needed <= gpu_usable_after_buffer_gb) && (kv_cache_gb <= ram_usable_after_buffer_gb);
    let ping_pong_penalty_ms = (total_layers as f64) * (((gpu_pcie_roundtrip_us / 1_000_000.0) * 2.0) + (0.00004 / gpu_pcie_bw)) * 1000.0;
    let b_gen_ms = (gpu_gen_ms * 0.65) + (cpu_gen_ms * 0.35) + ping_pong_penalty_ms;
    let b_tps = if b_gen_ms > 0.0 { (1000.0 / b_gen_ms * 10.0).round() / 10.0 } else { 0.0 };
    let b_ttft = (gpu_prefill_total_ms + ping_pong_penalty_ms * 2.0 * 10.0).round() / 10.0;
    candidates.push(ExecutionCandidate {
        id: "candidate_b_weights_gpu_kv_ram".to_string(),
        name: "Candidate B: Weights GPU, KV in Host RAM (offload_kqv=false)".to_string(),
        feasible: b_feasible,
        ineligibility_reason: if b_feasible {
            None
        } else if !has_gpu {
            Some("No CUDA GPU detected".into())
        } else if b_vram_needed > gpu_usable_after_buffer_gb {
            Some(format!("Model weights + workspace ({:.2} GB) exceed usable VRAM ({:.2} GB)", b_vram_needed, gpu_usable_after_buffer_gb))
        } else {
            Some("Host RAM cannot accommodate KV cache".into())
        },
        gpu_layers: if b_feasible { total_layers } else { 0 },
        cpu_layers: 0,
        total_layers,
        vram_required_gb: (b_vram_needed * 100.0).round() / 100.0,
        ram_required_gb: (kv_cache_gb * 100.0).round() / 100.0,
        kv_location: "RAM".into(),
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        prompt_ms_per_token: ((gpu_gen_ms * 0.70 + ping_pong_penalty_ms) * 100.0).round() / 100.0,
        generation_ms_per_token: (b_gen_ms * 100.0).round() / 100.0,
        generation_tps: if b_feasible { b_tps } else { 0.0 },
        ttft_ms: if b_feasible { b_ttft } else { 0.0 },
        is_winner: false,
        notes: "Attention executed on CPU. Incurs per-layer hidden state PCIe roundtrip transfers.".into(),
    });

    // Candidate C: Split Hybrid (Partial Offloading)
    let vram_per_layer = layer_size_gb + (kv_cache_gb / total_layers as f64);
    let max_gpu_layers = if has_gpu && vram_per_layer > 0.0 {
        ((gpu_usable_after_buffer_gb - compute_buffer_gb).max(0.0) / vram_per_layer).floor() as usize
    } else {
        0
    };
    let c_gpu_layers = max_gpu_layers.min(total_layers.saturating_sub(1));
    let c_cpu_layers = total_layers.saturating_sub(c_gpu_layers);
    let c_vram_needed = (c_gpu_layers as f64 * vram_per_layer) + compute_buffer_gb;
    let c_ram_needed = c_cpu_layers as f64 * (layer_size_gb + (kv_cache_gb / total_layers as f64));
    let c_feasible = has_gpu && c_gpu_layers >= 1 && c_cpu_layers >= 1 && (c_vram_needed <= gpu_usable_after_buffer_gb) && (c_ram_needed <= ram_usable_after_buffer_gb);
    let c_gpu_frac = c_gpu_layers as f64 / total_layers as f64;
    let c_cpu_frac = c_cpu_layers as f64 / total_layers as f64;
    let boundary_hop_ms = (gpu_pcie_roundtrip_us / 1000.0) * 1.5;
    let c_gen_ms = (c_gpu_frac * gpu_gen_ms) + (c_cpu_frac * cpu_gen_ms) + boundary_hop_ms;
    let c_tps = if c_gen_ms > 0.0 { (1000.0 / c_gen_ms * 10.0).round() / 10.0 } else { 0.0 };
    let c_ttft = (((c_gpu_frac * gpu_prefill_total_ms) + (c_cpu_frac * cpu_prefill_total_ms) + boundary_hop_ms * 2.0) * 10.0).round() / 10.0;
    candidates.push(ExecutionCandidate {
        id: "candidate_c_split_hybrid".to_string(),
        name: "Candidate C: Split Hybrid (Partial Offload)".to_string(),
        feasible: c_feasible,
        ineligibility_reason: if c_feasible {
            None
        } else if !has_gpu {
            Some("No CUDA GPU detected".into())
        } else if a_feasible {
            Some("Entire model already fits in VRAM (Candidate A is strictly superior)".into())
        } else if c_gpu_layers < 1 {
            Some("VRAM insufficient to hold even a single layer with compute workspace".into())
        } else {
            Some(format!("Host RAM ({:.2} GB needed) exceeds usable Host RAM ({:.2} GB)", c_ram_needed, ram_usable_after_buffer_gb))
        },
        gpu_layers: c_gpu_layers,
        cpu_layers: c_cpu_layers,
        total_layers,
        vram_required_gb: (c_vram_needed * 100.0).round() / 100.0,
        ram_required_gb: (c_ram_needed * 100.0).round() / 100.0,
        kv_location: "VRAM + RAM (per-layer)".into(),
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        prompt_ms_per_token: (((c_gpu_frac * (vram_read_ms / batch_tokens as f64)) + (c_cpu_frac * (ram_read_ms / batch_tokens as f64))) * 100.0).round() / 100.0,
        generation_ms_per_token: (c_gen_ms * 100.0).round() / 100.0,
        generation_tps: if c_feasible { c_tps } else { 0.0 },
        ttft_ms: if c_feasible { c_ttft } else { 0.0 },
        is_winner: false,
        notes: format!("Offloads {}/{} layers to GPU. RAM layers compute on CPU directly without per-token PCIe copying.", c_gpu_layers, total_layers),
    });

    // Candidate D: CPU Only (Host RAM SIMD)
    let d_ram_needed = weights_gb + kv_cache_gb + compute_buffer_gb;
    let d_feasible = d_ram_needed <= ram_usable_after_buffer_gb;
    let d_tps = if cpu_gen_ms > 0.0 { (1000.0 / cpu_gen_ms * 10.0).round() / 10.0 } else { 0.0 };
    let d_ttft = (cpu_prefill_total_ms * 10.0).round() / 10.0;
    candidates.push(ExecutionCandidate {
        id: "candidate_d_cpu_only".to_string(),
        name: "Candidate D: CPU Only (Host RAM SIMD)".to_string(),
        feasible: d_feasible,
        ineligibility_reason: if d_feasible {
            None
        } else {
            Some(format!("RAM footprint {:.2} GB exceeds usable Host RAM {:.2} GB", d_ram_needed, ram_usable_after_buffer_gb))
        },
        gpu_layers: 0,
        cpu_layers: total_layers,
        total_layers,
        vram_required_gb: 0.0,
        ram_required_gb: (d_ram_needed * 100.0).round() / 100.0,
        kv_location: "RAM".into(),
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        prompt_ms_per_token: (((ram_read_ms / batch_tokens as f64) + cpu_simd_ms) * 100.0).round() / 100.0,
        generation_ms_per_token: (cpu_gen_ms * 100.0).round() / 100.0,
        generation_tps: if d_feasible { d_tps } else { 0.0 },
        ttft_ms: if d_feasible { d_ttft } else { 0.0 },
        is_winner: false,
        notes: "Runs entirely on CPU using AVX2 SIMD instructions. Zero GPU resource contention.".into(),
    });

    // Candidate E: MoE Split (Active Experts in RAM)
    let (e_dense_gb, e_expert_gb, e_active_expert_gb) = if is_moe {
        if let Some(ref path) = resolved_path {
            if let Ok(arch) = crate::metadata::gguf_prober::GgufBinaryProber::probe(path) {
                let dense = arch.total_dense_bytes as f64 / GIB;
                let expert = arch.total_expert_bytes as f64 / GIB;
                let active_ratio = if arch.expert_count > 0 {
                    (arch.expert_used_count as f64 / arch.expert_count as f64).clamp(0.01, 1.0)
                } else {
                    8.0 / 64.0
                };
                (dense, expert, expert * active_ratio)
            } else {
                (weights_gb * 0.15, weights_gb * 0.85, weights_gb * 0.85 * (8.0 / 64.0))
            }
        } else {
            (weights_gb * 0.15, weights_gb * 0.85, weights_gb * 0.85 * (8.0 / 64.0))
        }
    } else {
        (0.0, 0.0, 0.0)
    };

    let e_vram_needed = e_dense_gb + kv_cache_gb + compute_buffer_gb;
    let e_ram_needed = e_expert_gb;
    let e_feasible = is_moe && has_gpu && (e_vram_needed <= gpu_usable_after_buffer_gb) && (e_ram_needed <= ram_usable_after_buffer_gb);

    let e_time_expert_read_ms = if ram_read_bw > 0.0 { (e_active_expert_gb / (ram_read_bw * 0.70)) * 1000.0 } else { 0.0 };
    let e_time_dense_gpu_ms = if gpu_vram_bw > 0.0 { (e_dense_gb / (gpu_vram_bw * 0.70)) * 1000.0 } else { 0.0 } + cuda_compute_ms;
    let e_gen_ms = e_time_expert_read_ms + e_time_dense_gpu_ms + 1.0;
    let e_tps = if e_gen_ms > 0.0 { (1000.0 / e_gen_ms * 10.0).round() / 10.0 } else { 0.0 };
    let e_ttft = (gpu_prefill_total_ms + (e_dense_gb / gpu_vram_bw.max(1.0)) * 1000.0 * 10.0).round() / 10.0;

    candidates.push(ExecutionCandidate {
        id: "candidate_e_moe_split".to_string(),
        name: "Candidate E: MoE Split (Active Experts in RAM)".to_string(),
        feasible: e_feasible,
        ineligibility_reason: if e_feasible {
            None
        } else if !is_moe {
            Some("Dense architecture (no MoE routed sparse experts detected)".into())
        } else if !has_gpu {
            Some("No CUDA GPU detected".into())
        } else if e_vram_needed > gpu_usable_after_buffer_gb {
            Some(format!("MoE dense backbone + KV ({:.2} GB) exceeds usable VRAM ({:.2} GB)", e_vram_needed, gpu_usable_after_buffer_gb))
        } else {
            Some(format!("MoE expert weights ({:.2} GB) exceed usable Host RAM ({:.2} GB)", e_ram_needed, ram_usable_after_buffer_gb))
        },
        gpu_layers: if e_feasible { total_layers } else { 0 },
        cpu_layers: 0,
        total_layers,
        vram_required_gb: if is_moe { (e_vram_needed * 100.0).round() / 100.0 } else { 0.0 },
        ram_required_gb: if is_moe { (e_ram_needed * 100.0).round() / 100.0 } else { 0.0 },
        kv_location: "VRAM".into(),
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        prompt_ms_per_token: ((gpu_prefill_total_ms / batch_tokens as f64) * 100.0).round() / 100.0,
        generation_ms_per_token: (e_gen_ms * 100.0).round() / 100.0,
        generation_tps: if e_feasible { e_tps } else { 0.0 },
        ttft_ms: if e_feasible { e_ttft } else { 0.0 },
        is_winner: false,
        notes: "Dense backbone in VRAM; routed experts dynamically resolved from Host RAM.".into(),
    });

    // 7. Pick The Winner (Strictly require feasible && positive generation TPS)
    let mut best_idx = None;
    let mut best_tps = 0.0;

    for (idx, cand) in candidates.iter().enumerate() {
        if cand.feasible && cand.generation_tps > best_tps {
            best_tps = cand.generation_tps;
            best_idx = Some(idx);
        }
    }

    let (winner_id, winner_name, winner_tps, winner_ttft, winner_gpu_layers, winner_cpu_layers, speedup_ratio, decision_reason) = if let Some(idx) = best_idx {
        candidates[idx].is_winner = true;
        let w_id = candidates[idx].id.clone();
        let w_name = candidates[idx].name.clone();
        let w_tps = candidates[idx].generation_tps;
        let w_ttft = candidates[idx].ttft_ms;
        let w_gpu = candidates[idx].gpu_layers;
        let w_cpu = candidates[idx].cpu_layers;
        let cpu_tps = candidates[3].generation_tps.max(0.1);
        let ratio = (w_tps / cpu_tps * 100.0).round() / 100.0;
        let reason = if w_id == "candidate_a_full_gpu" {
            format!(
                "{} wins with {:.1} tokens/sec ({:.2}x faster than CPU {:.1} TPS, TTFT {:.1} ms). The {:.2} GB model fits entirely inside {:.2} GB VRAM (usable {:.2} GB), incurring 0 PCIe transfer penalties during token generation.",
                w_name, w_tps, ratio, cpu_tps, w_ttft, weights_gb, gpu_total_vram_gb, gpu_usable_after_buffer_gb
            )
        } else if w_id == "candidate_c_split_hybrid" {
            format!(
                "{} wins with {:.1} tokens/sec ({:.2}x faster than CPU {:.1} TPS, TTFT {:.1} ms). Offloading {}/{} layers to GPU accelerates compute while remaining layers execute directly on CPU without per-token PCIe streaming.",
                w_name, w_tps, ratio, cpu_tps, w_ttft, candidates[2].gpu_layers, total_layers
            )
        } else if w_id == "candidate_e_moe_split" {
            format!(
                "{} wins with {:.1} tokens/sec ({:.2}x faster than CPU {:.1} TPS, TTFT {:.1} ms). Dense backbone ({:.2} GB) offloaded to GPU VRAM while routed experts ({:.2} GB) execute in Host RAM.",
                w_name, w_tps, ratio, cpu_tps, w_ttft, e_dense_gb, e_expert_gb
            )
        } else {
            format!(
                "{} selected ({:.1} tokens/sec, TTFT {:.1} ms). Model execution runs in Host RAM using CPU SIMD compute.",
                w_name, w_tps, w_ttft
            )
        };
        (w_id, w_name, w_tps, w_ttft, w_gpu, w_cpu, ratio, reason)
    } else {
        (
            "none".to_string(),
            "None (Infeasible)".to_string(),
            0.0,
            0.0,
            0,
            0,
            0.0,
            format!("NO_FEASIBLE_EXECUTION_PLAN: Model footprint ({:.2} GB + KV {:.2} GB) exceeds usable hardware capacity (Usable VRAM: {:.2} GB, Usable RAM: {:.2} GB).",
                weights_gb, kv_cache_gb, gpu_usable_after_buffer_gb, ram_usable_after_buffer_gb)
        )
    };

    // Calculate exact free memory required to load winner without crashing
    let vram_free_needed_gb = if winner_id == "candidate_a_full_gpu" {
        ((a_required_vram + vram_safety_buffer_gb) * 100.0).round() / 100.0
    } else if winner_id == "candidate_c_split_hybrid" {
        ((c_vram_needed + vram_safety_buffer_gb) * 100.0).round() / 100.0
    } else if winner_id == "candidate_e_moe_split" {
        ((e_vram_needed + vram_safety_buffer_gb) * 100.0).round() / 100.0
    } else {
        0.0
    };
    let ram_free_needed_gb = if winner_id == "candidate_a_full_gpu" {
        (ram_safety_buffer_gb * 100.0).round() / 100.0
    } else if winner_id == "candidate_c_split_hybrid" {
        ((c_ram_needed + ram_safety_buffer_gb) * 100.0).round() / 100.0
    } else if winner_id == "candidate_e_moe_split" {
        ((e_ram_needed + ram_safety_buffer_gb) * 100.0).round() / 100.0
    } else {
        ((d_ram_needed + ram_safety_buffer_gb) * 100.0).round() / 100.0
    };

    let faster_target = match winner_id.as_str() {
        "candidate_a_full_gpu" => "GPU_VRAM (Candidate A: Full GPU)".to_string(),
        "candidate_b_weights_gpu_kv_ram" => "GPU_KV_RAM (Candidate B: Weights GPU, KV RAM)".to_string(),
        "candidate_c_split_hybrid" => "GPU_SPLIT (Candidate C: Split Hybrid)".to_string(),
        "candidate_e_moe_split" => "GPU_MOE (Candidate E: MoE Split)".to_string(),
        _ => "CPU (Candidate D: CPU Only)".to_string(),
    };

    let breakdown = format!(
        "Model: {:.2} GB ({} layers, ctx: {}) | Winner: {} @ {:.1} TPS (TTFT: {:.1} ms, {:.2}x vs CPU) | VRAM: {:.2}/{:.2} GB, RAM: {:.2}/{:.2} GB",
        weights_gb, total_layers, n_ctx, winner_name, winner_tps, winner_ttft, speedup_ratio,
        gpu_used_vram_gb, gpu_total_vram_gb, used_ram_gb, total_ram_gb
    );

    let final_model_path = resolved_path.map(|p| p.to_string_lossy().to_string()).or_else(|| req.model_path.as_ref().map(|p| p.to_string_lossy().to_string()));

    // Dynamic Model Capabilities & Modalities derived from model_registry.json tasks & category
    let max_input_tokens = if reg_context > 0 { reg_context.max(n_ctx) } else { n_ctx };
    let max_output_tokens = 4096.min(max_input_tokens);

    let slot = crate::metadata::SlotType::from_category(&reg_category);
    let has_vision = reg_tasks.iter().any(|t| t == "document-ocr" || t == "spatial-vision" || t == "table-extraction" || t == "vision" || t == "image-to-text") 
        || slot == crate::metadata::SlotType::Ingest;
    let supported_inputs = slot.supported_inputs(has_vision);
    let supported_outputs = slot.supported_outputs();
    let supported_attachments = slot.supported_attachments(has_vision);
    let supported_modalities = supported_inputs.clone();

    // ─── Clean Unified Schema Construction ───
    let clean_model_info = ModelPlanInfo {
        model_id: resolved_model_name.clone(),
        architecture: architecture.clone(),
        model_size_gb: (weights_gb * 100.0).round() / 100.0,
        total_layers,
        attention_heads: head_count,
        kv_heads: head_count_kv,
        head_dim,
        max_input_context: max_input_tokens,
        max_output_context: max_output_tokens,
        active_working_context: n_ctx,
        inputs: supported_inputs.clone(),
        outputs: supported_outputs.clone(),
        supported_attachments: supported_attachments.clone(),
    };

    let placement_mode = if winner_id == "none" {
        "infeasible".to_string()
    } else if winner_id == "candidate_a_full_gpu" {
        "gpu_only".to_string()
    } else if winner_id == "candidate_c_split_hybrid" {
        "hybrid_gpu_ram".to_string()
    } else if winner_id == "candidate_d_cpu_only" {
        "cpu_only".to_string()
    } else if winner_id == "candidate_e_moe_split" {
        "hybrid_moe_gpu_ram".to_string()
    } else {
        "hybrid_gpu_ram".to_string()
    };

    let kv_cache_location = if winner_id == "candidate_a_full_gpu" {
        "gpu_vram".to_string()
    } else if winner_id == "candidate_b_weights_gpu_kv_ram" {
        "host_ram".to_string()
    } else if winner_id == "candidate_c_split_hybrid" {
        "gpu_vram".to_string()
    } else if winner_id == "candidate_d_cpu_only" {
        "host_ram".to_string()
    } else {
        "none".to_string()
    };

    let ram_layers_compute_target = if winner_cpu_layers > 0 {
        "cpu_avx2_simd".to_string()
    } else {
        "none".to_string()
    };

    let (model_in_vram_gb, model_in_ram_gb) = if winner_id == "candidate_a_full_gpu" {
        ((weights_gb * 100.0).round() / 100.0, 0.0)
    } else if winner_id == "candidate_c_split_hybrid" {
        let layer_w = weights_gb / (total_layers.max(1) as f64);
        (((layer_w * winner_gpu_layers as f64) * 100.0).round() / 100.0,
         ((layer_w * winner_cpu_layers as f64) * 100.0).round() / 100.0)
    } else if winner_id == "candidate_d_cpu_only" {
        (0.0, (weights_gb * 100.0).round() / 100.0)
    } else {
        (0.0, 0.0)
    };

    let kv_cache_memory_gb = (kv_cache_gb * 100.0).round() / 100.0;
    let remaining_free_vram_gb = ((gpu_free_vram_gb - model_in_vram_gb - if kv_cache_location == "gpu_vram" { kv_cache_memory_gb } else { 0.0 }).max(0.0) * 100.0).round() / 100.0;
    let remaining_free_ram_gb = ((free_ram_gb - model_in_ram_gb - if kv_cache_location == "host_ram" { kv_cache_memory_gb } else { 0.0 }).max(0.0) * 100.0).round() / 100.0;

    let gpu_speed_ms_per_layer = ((gpu_gen_ms / total_layers.max(1) as f64) * 1000.0).round() / 1000.0;
    let cpu_speed_ms_per_layer = ((cpu_gen_ms / total_layers.max(1) as f64) * 1000.0).round() / 1000.0;
    let gpu_to_cpu_power_ratio = ((cpu_speed_ms_per_layer / gpu_speed_ms_per_layer.max(0.001)) * 100.0).round() / 100.0;
    let calculation_note = format!("GPU is {:.2}x faster per layer than CPU on this hardware", gpu_to_cpu_power_ratio);

    let gpu_execution_time_ms = ((winner_gpu_layers as f64 * gpu_speed_ms_per_layer) * 100.0).round() / 100.0;
    let ram_to_cpu_execution_time_ms = ((winner_cpu_layers as f64 * cpu_speed_ms_per_layer) * 100.0).round() / 100.0;
    let ram_to_gpu_transfer_time_ms = if winner_cpu_layers > 0 && winner_gpu_layers > 0 {
        0.18
    } else {
        0.0
    };
    let total_time_per_token_ms = ((gpu_execution_time_ms + ram_to_cpu_execution_time_ms + ram_to_gpu_transfer_time_ms) * 100.0).round() / 100.0;

    let clean_execution_plan = ExecutionStrategyPlan {
        placement_mode,
        gpu_layers: winner_gpu_layers,
        ram_layers: winner_cpu_layers,
        ssd_layers: if winner_id == "none" { total_layers } else { 0 },
        kv_cache_location,
        ram_layers_compute_target,
        model_memory_breakdown: MemoryBreakdown {
            model_in_vram_gb,
            model_in_ram_gb,
            kv_cache_memory_gb,
            remaining_free_vram_gb,
            remaining_free_ram_gb,
        },
        compute_capability_rates: ComputeCapabilityRates {
            gpu_speed_ms_per_layer,
            cpu_speed_ms_per_layer,
            gpu_to_cpu_power_ratio,
            compute_winner: if gpu_to_cpu_power_ratio >= 1.0 { "GPU".to_string() } else { "CPU".to_string() },
            summary: calculation_note,
        },
        bus_timings: BusTimings {
            total_time_per_token_ms,
            gpu_execution_time_ms,
            ram_to_cpu_execution_time_ms,
            ram_to_gpu_transfer_time_ms,
            ram_to_gpu_sync_delay_us: gpu_pcie_roundtrip_us,
        },
    };

    let (ssd_type, ssd_speed_mbps) = if let Some(s) = control.silicon_truth.storage.iter().find(|s| s.is_primary_workspace).or_else(|| control.silicon_truth.storage.first()) {
        (
            if s.drive_type.contains("NVMe") || s.drive_type.contains("SSD") { s.drive_type.clone() } else { "NVMe SSD".to_string() },
            if s.read_speed_mbps > 0.0 { (s.read_speed_mbps * 10.0).round() / 10.0 } else { 3500.0 }
        )
    } else {
        ("NVMe SSD".to_string(), 3500.0)
    };

    let clean_hardware_info = HardwareSnapshotInfo {
        gpu: GpuHardwareInfo {
            model: gpu_model.clone(),
            total_vram_gb: (gpu_total_vram_gb * 100.0).round() / 100.0,
            free_vram_gb: (gpu_free_vram_gb * 100.0).round() / 100.0,
            safety_buffer_gb: (vram_safety_buffer_gb * 100.0).round() / 100.0,
            usable_vram_gb: (gpu_usable_after_buffer_gb * 100.0).round() / 100.0,
            vram_speed_gbps: (gpu_vram_bw * 100.0).round() / 100.0,
            compute_capability: if let Some(g) = maybe_gpu {
                g.compute_capability.clone().unwrap_or_else(|| "8.9".to_string())
            } else {
                "8.9".to_string()
            },
        },
        cpu: CpuHardwareInfo {
            model: cpu_brand.trim().to_string(),
            cores: cpu_cores,
            threads: cpu_threads,
            clock_ghz: (cpu_clock_ghz * 10.0).round() / 10.0,
            peak_gflops: (cpu_peak_gflops * 10.0).round() / 10.0,
            current_load_pct: (cpu_util_pct as f64 * 10.0).round() / 10.0,
        },
        ram: RamHardwareInfo {
            total_ram_gb: (total_ram_gb * 100.0).round() / 100.0,
            free_ram_gb: (free_ram_gb * 100.0).round() / 100.0,
            safety_buffer_gb: (ram_safety_buffer_gb * 100.0).round() / 100.0,
            usable_ram_gb: (ram_usable_after_buffer_gb * 100.0).round() / 100.0,
            read_speed_gbps: (ram_read_bw * 10.0).round() / 10.0,
            access_latency_ns: (ram_latency_ns * 10.0).round() / 10.0,
        },
        workspace_buffer: WorkspaceBufferInfo {
            compute_workspace_gb: (compute_buffer_gb * 100.0).round() / 100.0,
            purpose: "Matrix multiplication scratch memory".to_string(),
        },
        storage_ssd: StorageSsdInfo {
            drive_type: ssd_type,
            speed_mbps: ssd_speed_mbps,
        },
        gpu_pcie_bus: PcieBusInfo {
            bus_type: if let Some(g) = maybe_gpu {
                if !g.connection.is_empty() {
                    g.connection.clone()
                } else if g.pcie_link_gen > 0 {
                    format!("PCIe Gen {}.0 x{}", g.pcie_link_gen, g.pcie_link_width)
                } else {
                    "PCIe Gen 4.0 x4".to_string()
                }
            } else {
                "PCIe Gen 4.0 x4".to_string()
            },
            transfer_speed_gbps: (gpu_pcie_bw * 10.0).round() / 10.0,
            bus_latency_us: (gpu_pcie_roundtrip_us * 10.0).round() / 10.0,
            description: "High-speed connection highway between System RAM and GPU VRAM".to_string(),
        },
    };

    // ─── Active User Settings Construction (from live configs) ───
    let perm_path = env.config_dir().join("permission.json");
    let detected_slot: Option<String> = if perm_path.exists() {
        std::fs::read_to_string(&perm_path).ok().and_then(|c| {
            let val: serde_json::Value = serde_json::from_str(&c).ok()?;
            let slots = val.get("active_slots")?.as_object()?;
            for (slot_name, slot_val) in slots {
                if let Some(m_id) = slot_val.get("model_id").and_then(|v| v.as_str()) {
                    if let Some(ref target) = requested_id {
                        let clean_t = target.to_lowercase().replace([':', '_', '.'], "-");
                        let clean_m = m_id.to_lowercase().replace([':', '_', '.'], "-");
                        if clean_m.contains(&clean_t) || clean_t.contains(&clean_m) {
                            return Some(slot_name.clone());
                        }
                    }
                }
            }
            None
        })
    } else {
        None
    };

    let active_user_settings = ActiveUserSettings {
        target_slot: detected_slot,
        n_gpu_layers: gguf_config.hardware_and_execution.n_gpu_layers,
        n_ctx,
        batch_size: gguf_config.hardware_and_execution.batch_size,
        ubatch_size: gguf_config.hardware_and_execution.ubatch_size,
        flash_attention: format!("{:?}", opt_control.flash_attention),
        kv_cache_quantization: format!("{:?}", opt_control.kv_cache_quantization),
        hybrid_memory: format!("{:?}", opt_control.hybrid_memory),
        vram_buffer_gb: (vram_safety_buffer_gb * 100.0).round() / 100.0,
        ram_buffer_gb: (ram_safety_buffer_gb * 100.0).round() / 100.0,
    };

    Ok(HardwareExecutionComparison {
        active_user_settings,
        model_info: clean_model_info,
        execution_plan: clean_execution_plan,
        hardware_info: clean_hardware_info,
        model_name: Some(resolved_model_name),
        architecture: Some(architecture),
        weights_gb,
        batch_tokens,
        total_layers,
        n_ctx,
        max_input_tokens,
        max_output_tokens,
        supported_inputs,
        supported_outputs,
        supported_attachments,
        supported_modalities,
        kv_cache_gb: (kv_cache_gb * 100.0).round() / 100.0,
        compute_buffer_gb: (compute_buffer_gb * 100.0).round() / 100.0,
        model_path: final_model_path,
        vram_safety_buffer_gb,
        ram_safety_buffer_gb,
        vram_free_needed_gb,
        ram_free_needed_gb,
        gpu_layers: winner_gpu_layers,
        cpu_layers: winner_cpu_layers,
        model_fits_in_vram: a_feasible,
        faster_target,
        speedup_ratio,
        cpu_latency_ms: (cpu_gen_ms * 100.0).round() / 100.0,
        gpu_pcie_latency_ms: (pcie_streaming_ms * 100.0).round() / 100.0,
        gpu_vram_latency_ms: if a_feasible { Some((gpu_gen_ms * 100.0).round() / 100.0) } else { None },
        decision_reason,
        breakdown,
        timing_breakdown,
        bus_ping_pong_latency,
        hardware_state,
        candidates,
        winner_candidate_id: winner_id,
        winner_generation_tps: winner_tps,
        winner_ttft_ms: winner_ttft,
    })
}
