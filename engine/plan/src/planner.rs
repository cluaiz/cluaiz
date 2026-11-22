//! 🎯 Placement planner: (model geometry, live hardware, settings) -> ExecutionPlan.
//! Follows the rules in resource_negotiator's header: CTX rules, BUFFER rules, STEP 1-3.
//! All byte counts come from the GGUF tensor table (`ModelGeometry::weights`) and llama.cpp's KV math.
//! STEP 4: a placement measured faster by `cluaiz bench` replaces the rule-order pick ([`crate::speed`]).

use cluaiz_core::{
    bytes_to_gib, gib_to_bytes, ActiveUserSettings, ContextSetting, KvCacheQuantization,
    DEFAULT_SAFE_VRAM_BUFFER_GB, MIN_SAFE_VRAM_BUFFER_GB,
};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;
use crate::types::{ExecutionCandidate, ExecutionPlan, PlacementTier};

/// CTX rule 3: Auto gives at least this many tokens (or the model's native limit if smaller) when
/// the model runs fully on the GPU, and exactly this many when it has to be split.
pub const AUTO_CONTEXT_FLOOR: u32 = 4096;
/// CTX rule 3a: when the floor does not fit fully on the GPU, Auto takes this many tokens on the
/// GPU before it splits the model (speed first: a smaller context before RAM).
pub const AUTO_CONTEXT_MIN: u32 = 2048;
/// Auto on the GPU stops here: enough for chats and agent work, and the rest of the VRAM stays
/// free for other models (Max or a number gives more)
pub const AUTO_CONTEXT_TARGET: u32 = 16384;
/// Auto context of a model split between GPU and RAM: twice the floor costs about one block on the
/// GPU, and a 4096-token context is used up by an agent's tools and one file
pub const AUTO_SPLIT_CONTEXT: u32 = 8192;
/// BUFFER rule: Cluaiz may use up to this share of total RAM, other apps included (>= 5% always free).
pub const RAM_CEILING_FRACTION: f64 = 0.95;
/// The RAM that share keeps free stops growing here (5% of 256 GB would be 12.8 GB): the OS and the
/// apps it is kept for do not grow with the machine
pub const RAM_RESERVE_MAX_GB: f64 = 4.0;
/// llama.cpp pads the context to 256 cells, so planned sizes stay on that grid.
const CONTEXT_ALIGN: u32 = cluaiz_models::KV_CELL_PAD;

/// Memory one placement has to fit into.
pub struct DeviceBudget {
    pub free_bytes: u64,
    /// Bytes that must sit on this device besides the KV cache (weights + logits buffer)
    pub fixed_bytes: u64,
    /// Buffer candidates in GB, tried in order
    pub buffers_gb: Vec<f64>,
}

/// Result of the CTX / BUFFER rules for one model on one device.
pub struct ContextChoice {
    pub n_ctx: u32,
    pub kv_quant: KvCacheQuantization,
    pub buffer_gb: f64,
    /// True when weights + KV + logits fit on the device with the chosen buffer.
    pub fits_on_device: bool,
    pub reason: String,
}

/// VRAM buffer candidates: the user's value, else 1024 MiB then 512 MiB (BUFFER rules).
pub fn vram_buffer_options(settings: &ActiveUserSettings) -> Vec<f64> {
    match settings.optimization.custom_vram_buffer_gb {
        Some(custom) => vec![custom],
        None => vec![DEFAULT_SAFE_VRAM_BUFFER_GB, MIN_SAFE_VRAM_BUFFER_GB],
    }
}

/// VRAM buffer candidates for a model: the user's value; else, once the model was measured on
/// this machine, what it really took over the estimate plus the desktop reserve
/// ([`crate::margin`]); else the BUFFER rules. A measured buffer is negative when the estimate was
/// higher than what the model took. A measurement no load can produce (below minus the whole GPU,
/// from a load that spilled into shared memory) is ignored. A model not measured yet whose estimate
/// llama.cpp's dry run corrected gets a gap sized by the card ([`crate::margin::card_gap_gb`]).
pub fn model_buffer_options(geometry: &ModelGeometry, hardware: &SystemInfo, settings: &ActiveUserSettings) -> Vec<f64> {
    let learned = crate::margin::learned(geometry, settings).filter(|gap| crate::margin::plausible_gap(*gap, hardware));
    let corrected = crate::margin::correction(geometry, settings).is_some() && !hardware.gpus.is_empty();
    match (settings.optimization.custom_vram_buffer_gb, learned) {
        (None, Some(gap)) => vec![gap + crate::margin::reserve_gb(hardware)],
        (None, None) if corrected => vec![crate::margin::card_gap_gb(hardware)],
        _ => vram_buffer_options(settings),
    }
}

/// Bytes a placement may use of `free` with `buffer_gb` kept back. Never more than `free`: a
/// negative buffer (an estimate measured too high) only removes the margin, it does not add VRAM
/// the GPU does not have.
fn usable_bytes(free: u64, buffer_gb: f64) -> u64 {
    free.saturating_sub(gib_to_bytes(buffer_gb.max(0.0)))
}

/// RAM Cluaiz may use: free RAM minus 5% of total RAM (at most [`RAM_RESERVE_MAX_GB`]), or free RAM
/// minus the user's GB buffer when set (BUFFER rules: "GB values set by the user replace both").
pub fn ram_budget_bytes(hardware: &SystemInfo, settings: &ActiveUserSettings) -> u64 {
    let total = hardware.memory.total_gb;
    let free = hardware.memory.available_gb;
    let budget_gb = match settings.optimization.custom_ram_buffer_gb {
        Some(buffer) => free - buffer,
        None => free - (total * (1.0 - RAM_CEILING_FRACTION)).min(RAM_RESERVE_MAX_GB),
    };
    gib_to_bytes(budget_gb.max(0.0))
}

/// CTX rules (Custom N exact, Max native, Auto largest-that-fits with floor 4096, else 2048 on the
/// GPU) and BUFFER rules. With KV setting Auto, F16 is tried before Q8_0.
pub fn resolve_context(geometry: &ModelGeometry, budget: &DeviceBudget, settings: &ActiveUserSettings) -> ContextChoice {
    let opt = &settings.optimization;
    let ubatch = opt.resolved_ubatch_size();
    let native_ctx = geometry.context_length.max(1);
    let floor_ctx = native_ctx.min(AUTO_CONTEXT_FLOOR);
    let min_ctx = native_ctx.min(AUTO_CONTEXT_MIN);

    let kv_options: Vec<KvCacheQuantization> = match opt.kv_cache_quantization {
        KvCacheQuantization::Auto => vec![KvCacheQuantization::Kv16, KvCacheQuantization::Kv8],
        fixed => vec![fixed],
    };
    let most_compact = *kv_options.last().unwrap();
    let first_buffer = budget.buffers_gb.first().copied().unwrap_or(0.0);
    let fits = |ctx: u32, kv: KvCacheQuantization, buf_gb: f64| {
        budget.fixed_bytes + geometry.kv_cache_bytes(ctx, kv, ubatch) <= usable_bytes(budget.free_bytes, buf_gb)
    };

    let exact = |ctx: u32, label: &str| -> ContextChoice {
        for &kv in &kv_options {
            for &buf in &budget.buffers_gb {
                if fits(ctx, kv, buf) {
                    return ContextChoice {
                        n_ctx: ctx, kv_quant: kv, buffer_gb: buf, fits_on_device: true,
                        reason: format!("{} {} tokens, KV {:?}, buffer {:.2} GB", label, ctx, kv, buf),
                    };
                }
            }
        }
        ContextChoice {
            n_ctx: ctx, kv_quant: most_compact, buffer_gb: first_buffer, fits_on_device: false,
            reason: format!("{} {} tokens kept (never cut), KV {:?}; does not fit on one device", label, ctx, most_compact),
        }
    };

    match opt.context_setting() {
        ContextSetting::Custom(n) => exact(n.min(native_ctx), "Custom"),
        ContextSetting::Max => exact(native_ctx, "Max (native)"),
        ContextSetting::Auto => {
            // The model stays on the GPU first; the KV cache gets smaller before the context does:
            //   1. KV F16 when the whole target fits
            //   2. KV Q8 (near lossless), the largest context up to the target, at least the floor
            //   3. KV Q4, the floor (only to stay on the GPU, so it never gives more than Q8 would)
            //   4. CTX rule 3a: below the floor down to AUTO_CONTEXT_MIN, most compact KV
            //   5. split between GPU and RAM
            // Each step tries the buffers in order. A step that needs less memory never gives a bigger
            // context than the one before it, so more free VRAM never gives a worse plan.
            // A KV type the user set is the only one tried.
            let target = native_ctx.min(AUTO_CONTEXT_TARGET);
            let auto_kv = opt.kv_cache_quantization == KvCacheQuantization::Auto;
            let steps: Vec<(KvCacheQuantization, u32, u32)> = if auto_kv {
                vec![
                    (KvCacheQuantization::Kv16, target, target),
                    (KvCacheQuantization::Kv8, target, floor_ctx),
                    (KvCacheQuantization::Kv4, floor_ctx, floor_ctx),
                ]
            } else {
                vec![(most_compact, target, floor_ctx)]
            };
            let ladder_compact = steps.last().map(|s| s.0).unwrap_or(most_compact);
            // The first buffer runs the ladder; a smaller fallback buffer (512 MiB) only reaches the
            // floor, or less free memory could give a bigger context than more
            let mut tries: Vec<(KvCacheQuantization, u32, u32, f64)> =
                steps.iter().map(|&(kv, ceiling, least)| (kv, ceiling, least, first_buffer)).collect();
            for &buf in budget.buffers_gb.iter().skip(1) {
                tries.extend(steps.iter().map(|&(kv, _, _)| (kv, floor_ctx, floor_ctx, buf)));
            }
            for (i, &(kv, ceiling, least, buf)) in tries.iter().enumerate() {
                let ctx = largest_fitting_context(ceiling, |c| fits(c, kv, buf));
                if ctx >= least {
                    return ContextChoice {
                        n_ctx: ctx, kv_quant: kv, buffer_gb: buf, fits_on_device: true,
                        reason: if i == 0 {
                            format!("Auto: {} tokens, KV {:?}, buffer {:.2} GB", ctx, kv, buf)
                        } else {
                            format!("Auto: {} tokens with KV {:?} + buffer {:.2} GB, to stay on the GPU ({} tokens with KV {:?} did not fit)",
                                ctx, kv, buf, steps[0].1, steps[0].0)
                        },
                    };
                }
            }
            // CTX rule 3a: below the floor, down to AUTO_CONTEXT_MIN, while the model still runs
            // fully on the GPU (most compact KV: the point is to stay on the GPU)
            for &buf in &budget.buffers_gb {
                let ctx = largest_fitting_context(floor_ctx, |c| fits(c, ladder_compact, buf));
                if ctx >= min_ctx {
                    return ContextChoice {
                        n_ctx: ctx, kv_quant: ladder_compact, buffer_gb: buf, fits_on_device: true,
                        reason: format!("Auto: {} tokens, KV {:?} + buffer {:.2} GB, to stay on the GPU ({} did not fit)",
                            ctx, ladder_compact, buf, floor_ctx),
                    };
                }
            }
            // The model is split (or on the CPU): a fixed context, KV Q8 (each GPU block keeps its KV,
            // so a smaller KV would buy little speed for its quality)
            let split_ctx = native_ctx.min(AUTO_SPLIT_CONTEXT);
            let split_kv = if auto_kv { KvCacheQuantization::Kv8 } else { most_compact };
            ContextChoice {
                n_ctx: split_ctx, kv_quant: split_kv, buffer_gb: first_buffer, fits_on_device: false,
                reason: format!("Auto: {} tokens, KV {:?}; the model does not fit on one device, so it is split",
                    split_ctx, split_kv),
            }
        }
    }
}

/// Largest context <= `ceiling` for which `fits` holds: the ceiling itself, else the largest
/// multiple of 256. KV grows with context, so `fits` is monotonic and a binary search is exact.
fn largest_fitting_context(ceiling: u32, fits: impl Fn(u32) -> bool) -> u32 {
    if fits(ceiling) {
        return ceiling;
    }
    let (mut lo, mut hi) = (0u32, ceiling / CONTEXT_ALIGN);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if fits(mid * CONTEXT_ALIGN) { lo = mid } else { hi = mid - 1 }
    }
    lo * CONTEXT_ALIGN
}

pub struct Planner;

/// [`Planner::after_low_vram`]: keep the loaded plan, load a smaller one, or there is none smaller
#[derive(Debug)]
pub enum StepDown {
    Keep,
    Smaller(Box<ExecutionPlan>),
    NoSmaller,
}

/// Bytes of one llama.cpp placement (weights + KV + logits buffer per device).
struct Placement {
    vram: u64,
    ram: u64,
    kv_gpu: u64,
    kv_ram: u64,
    /// Weights alone, per device (the rest is KV and buffers)
    vram_weights: u64,
    ram_weights: u64,
}

impl Planner {
    /// The next placement after `failed` did not load (llama.cpp could not allocate it on the GPU),
    /// or None when there is none to try: the plan put nothing on a GPU, the user fixed the GPU
    /// layers or MoE offload, or the plan does not change. The miss is recorded first (it needed at
    /// least the `free_gb` that was free), so this model and these settings keep that much more free
    /// from now on; the caller saves the measurements.
    pub fn after_failed_load(
        failed: &ExecutionPlan,
        geometry: &ModelGeometry,
        free_gb: f64,
        live: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> Option<ExecutionPlan> {
        let manual = settings.optimization.n_gpu_layers.is_some() || settings.optimization.n_cpu_moe.is_some();
        if failed.n_gpu_layers == 0 || live.gpus.is_empty() || manual {
            return None;
        }
        crate::margin::record(geometry, settings, free_gb - failed.vram_budget_gb);
        let next = Self::create_plan(&failed.model_id, geometry, live, settings);
        let changed = next.n_gpu_layers != failed.n_gpu_layers
            || next.context_window != failed.context_window
            || next.n_cpu_moe != failed.n_cpu_moe
            || next.kv_cache_type != failed.kv_cache_type
            || next.kv_offload != failed.kv_offload
            || next.moe_cache_bytes != failed.moe_cache_bytes;
        (next.is_feasible() && changed).then_some(next)
    }

    /// What to do after a load that left `free_after_gb` of VRAM free. Less than
    /// [`crate::margin::DEVICE_RESERVE_MIN_GB`] means the load took more than planned (the load has
    /// recorded what it took): planned again on the free memory the load was planned with
    /// (`planned_on`), that gives a smaller placement. The same plan, or a larger one, is never
    /// loaded again.
    pub fn after_low_vram(
        loaded: &ExecutionPlan,
        geometry: &ModelGeometry,
        free_after_gb: f64,
        planned_on: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> StepDown {
        if loaded.n_gpu_layers == 0 || planned_on.gpus.is_empty() || free_after_gb >= crate::margin::DEVICE_RESERVE_MIN_GB {
            return StepDown::Keep;
        }
        let next = Self::create_plan(&loaded.model_id, geometry, planned_on, settings);
        if next.is_feasible() && next.vram_budget_gb + 0.01 < loaded.vram_budget_gb {
            StepDown::Smaller(Box::new(next))
        } else {
            StepDown::NoSmaller
        }
    }

    pub fn create_plan(
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let plan = Self::plan_with_gap(model_id, geometry, hardware, settings);
        Self::tight_gap(plan, model_id, geometry, hardware, settings)
    }

    /// A gap down to [`crate::margin::DEVICE_RESERVE_MIN_GB`] (plus the measured GPU pool, when the
    /// model was measured) when the usual gap leaves less of the model on the GPU: Full GPU instead
    /// of a split, the attention of every block instead of some of it, more blocks, or a GPU
    /// placement instead of CPU only; never one that reads more from disk. A Full GPU plan keeps the
    /// usual gap. A load that fails at the tight gap records the miss and steps down one placement
    /// (`after_failed_load`).
    fn tight_gap(
        plan: ExecutionPlan,
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let opt = &settings.optimization;
        if hardware.gpus.is_empty()
            || opt.n_gpu_layers == Some(0)
            || opt.custom_vram_buffer_gb.is_some()
            || opt.is_manual_placement()
            || plan.selected_tier == PlacementTier::GpuOnly
        {
            return plan;
        }
        let pool = crate::margin::learned(geometry, settings)
            .filter(|gap| crate::margin::plausible_gap(*gap, hardware))
            .unwrap_or(0.0)
            .max(0.0);
        let gap = pool + crate::margin::DEVICE_RESERVE_MIN_GB;
        let usual = model_buffer_options(geometry, hardware, settings).into_iter().fold(f64::MAX, f64::min);
        if gap + 1e-9 >= usual {
            return plan;
        }
        let mut tight = settings.clone();
        tight.optimization.custom_vram_buffer_gb = Some(gap);
        let mut other = Self::plan_with_gap(model_id, geometry, hardware, &tight);
        let more_on_gpu = (rank(&other), other.block_split().0) > (rank(&plan), plan.block_split().0);
        if other.is_feasible() && more_on_gpu && other.disk_stream_gb <= plan.disk_stream_gb + 0.01 {
            other.explanation = format!(
                "{}; {:.2} GB of VRAM kept free instead of {:.2} GB: with the usual gap it would be {}",
                other.explanation, gap, usual, plan.placement_label()
            );
            return other;
        }
        plan
    }

    pub(crate) fn plan_with_gap(
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        // A split model gets AUTO_SPLIT_CONTEXT; when RAM is too short for it, the floor
        let plan = Self::plan(model_id, geometry, hardware, settings, false);
        let floor_ctx = geometry.context_length.max(1).min(AUTO_CONTEXT_FLOOR);
        if !plan.is_feasible()
            && settings.optimization.context_setting() == ContextSetting::Auto
            && plan.context_window > floor_ctx
        {
            let smaller = Self::plan(model_id, geometry, hardware, settings, true);
            if smaller.is_feasible() {
                return smaller;
            }
        }
        Self::fewer_disk_reads(plan, model_id, geometry, hardware, settings)
    }

    /// Disk reads first: the context is chosen before the experts are placed, so an F16 KV cache
    /// can take the VRAM of expert layers and send them to the disk. With the KV type on Auto, a
    /// MoE plan that reads from disk is planned again with KV Q8, and the one reading less is kept.
    fn fewer_disk_reads(
        plan: ExecutionPlan,
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let kv_auto = settings.optimization.kv_cache_quantization == KvCacheQuantization::Auto;
        if plan.disk_stream_gb <= 0.0 || !kv_auto || settings.optimization.is_manual_placement() {
            return plan;
        }
        let mut best = plan;
        for kv in [KvCacheQuantization::Kv8] {
            if best.kv_cache_type == kv || best.disk_stream_gb <= 0.0 {
                continue;
            }
            let mut smaller_kv = settings.clone();
            smaller_kv.optimization.kv_cache_quantization = kv;
            let mut other = Self::plan(model_id, geometry, hardware, &smaller_kv, false);
            if other.is_feasible() && other.disk_stream_gb + 0.01 < best.disk_stream_gb {
                other.context_reason = format!(
                    "{}; KV {:?} instead of {:?} so {:.2} GB less of the experts is read from disk",
                    other.context_reason, kv, best.kv_cache_type, best.disk_stream_gb - other.disk_stream_gb
                );
                best = other;
            }
        }
        best
    }

    fn plan(
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
        split_at_floor: bool,
    ) -> ExecutionPlan {
        let opt = &settings.optimization;
        let ubatch = opt.resolved_ubatch_size();
        let w = &geometry.weights;
        let n_layers = geometry.block_count as usize;
        let logits = geometry.logits_buffer_bytes(ubatch);
        // llama.cpp's dry run of this model minus the estimate (crate::margin), counted with the
        // logits buffer: both are bytes the GPU takes besides the blocks and their KV. The dry run
        // has every weight on the GPU, so a placement gets the share of the correction that its GPU
        // weights are of all weights (a MoE split with 1.7 of 13.2 GB on the GPU gets 13% of it).
        let correction = crate::margin::correction(geometry, settings).unwrap_or(0.0);
        let full_weights = w.full_gpu().max(1);
        let gpu_logits_for = |gpu_weights: u64| -> u64 {
            let share = gpu_weights.min(full_weights) as f64 / full_weights as f64;
            let extra = correction * share;
            if extra >= 0.0 { logits + gib_to_bytes(extra) } else { logits.saturating_sub(gib_to_bytes(-extra)) }
        };
        let gpu_logits = gpu_logits_for(w.full_gpu());
        let experts = w.experts_total();
        let is_moe = experts > 0;

        let vram_free = gib_to_bytes(hardware.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0));
        let gpu_allowed = !hardware.gpus.is_empty() && opt.n_gpu_layers != Some(0);
        let ram_budget = ram_budget_bytes(hardware, settings);
        // Apple Silicon: the GPU's memory is the RAM, so what goes on the GPU is counted in RAM too, and
        // the GPU gets at most the free RAM (its own cap, the Metal working set, is still the VRAM limit)
        let unified = hardware.memory.is_unified_memory && !hardware.gpus.is_empty();

        // Context is decided for the preferred placement (CTX rules):
        // GPU -> dense: all weights on GPU, MoE: attention/shared weights on GPU; CPU -> everything in RAM.
        let budget = if gpu_allowed {
            let gpu_weights = if is_moe { w.full_gpu() - experts } else { w.full_gpu() };
            let free_bytes = if unified { vram_free.min(ram_budget.saturating_sub(w.input_cpu)) } else { vram_free };
            DeviceBudget {
                free_bytes,
                fixed_bytes: gpu_weights + gpu_logits_for(gpu_weights),
                buffers_gb: model_buffer_options(geometry, hardware, settings),
            }
        } else {
            DeviceBudget { free_bytes: ram_budget, fixed_bytes: w.total + logits, buffers_gb: vec![0.0] }
        };
        let mut choice = resolve_context(geometry, &budget, settings);
        // CTX rule 3 on the CPU too: Auto does not fill RAM with context (that RAM is what other
        // models and apps need); the floor, or less when only less fits
        let floor_ctx = geometry.context_length.max(1).min(AUTO_CONTEXT_FLOOR);
        if !gpu_allowed && opt.context_setting() == ContextSetting::Auto && choice.n_ctx > floor_ctx {
            choice.n_ctx = floor_ctx;
            choice.reason = format!("Auto on the CPU: {} tokens, KV {:?} (the floor; RAM is not filled with context)", floor_ctx, choice.kv_quant);
        } else if split_at_floor && !choice.fits_on_device && opt.context_setting() == ContextSetting::Auto {
            choice.n_ctx = floor_ctx;
            choice.reason = format!("Auto: {} tokens, KV {:?}; the model is split and RAM is too short for {} tokens",
                floor_ctx, choice.kv_quant, AUTO_SPLIT_CONTEXT);
        }
        let ctx = choice.n_ctx;
        let kv = choice.kv_quant;
        let vram_buffer_gb = if gpu_allowed { choice.buffer_gb } else { 0.0 };
        let vram_usable = usable_bytes(vram_free, vram_buffer_gb);
        let kv_total = geometry.kv_cache_bytes(ctx, kv, ubatch);

        // Bytes of a llama.cpp placement: n_gpu_layers (-1 = all blocks, N = output head + last N-1 blocks,
        // 0 = CPU), experts of the first n_cpu_moe blocks in RAM, KV next to its layer unless kv_offload is off.
        // Input embeddings always stay in RAM; every other non-block tensor goes with the output head.
        let block_bytes: u64 = w.layer.iter().sum::<u64>() + experts;
        let head_gpu = w.full_gpu().saturating_sub(block_bytes);
        let head_cpu = w.total.saturating_sub(w.input_cpu + block_bytes);
        let place = |n_gpu_layers: i32, n_cpu_moe: usize, kv_offload: bool| -> Placement {
            let gpu_blocks = match n_gpu_layers {
                n if n < 0 => n_layers,
                0 => 0,
                n => (n as usize - 1).min(n_layers),
            };
            let first_gpu_block = n_layers - gpu_blocks;
            let mut p = Placement { vram: 0, ram: w.input_cpu, kv_gpu: 0, kv_ram: 0, vram_weights: 0, ram_weights: 0 };
            if n_gpu_layers != 0 {
                p.vram += head_gpu;
            } else {
                p.ram += head_cpu + logits;
            }
            for l in 0..n_layers {
                let on_gpu = l >= first_gpu_block;
                let kv_l = geometry.kv_layer_bytes(l, ctx, kv, ubatch);
                if on_gpu { p.vram += w.layer[l] } else { p.ram += w.layer[l] }
                if on_gpu && l >= n_cpu_moe { p.vram += w.layer_experts[l] } else { p.ram += w.layer_experts[l] }
                if on_gpu && kv_offload { p.kv_gpu += kv_l } else { p.kv_ram += kv_l }
            }
            // Weights only so far: logits buffer and the share of the correction they call for
            p.vram_weights = p.vram;
            p.ram_weights = p.ram - if n_gpu_layers == 0 { logits } else { 0 };
            if n_gpu_layers != 0 {
                p.vram += gpu_logits_for(p.vram);
            }
            p.vram += p.kv_gpu;
            p.ram += p.kv_ram;
            if unified {
                p.ram += p.vram;
            }
            p
        };
        let over_vram = |p: &Placement| p.vram.saturating_sub(vram_usable);

        let mut candidates = Vec::new();

        // User fixed the placement (settings or the request's "llama" params): use exactly that, no refusal.
        // The estimate is still printed so the user sees what it costs.
        if opt.is_manual_placement() {
            // The user's n_gpu_layers goes to llama.cpp as it is, also when no GPU is detected here: the
            // probe sees NVIDIA GPUs only, so a Metal or Vulkan GPU would otherwise lose every layer
            let ngl = match opt.n_gpu_layers {
                Some(n) => n,
                None if hardware.gpus.is_empty() => 0,
                None => -1,
            };
            let n_cpu_moe = opt.n_cpu_moe.unwrap_or(0);
            let kv_offload = opt.offload_kqv.unwrap_or(true);
            let p = place(ngl, n_cpu_moe as usize, kv_offload);
            let tier = if ngl == 0 {
                PlacementTier::CpuOnly
            } else if n_cpu_moe > 0 {
                PlacementTier::MoeSplit
            } else if (ngl < 0 || ngl as usize > n_layers) && kv_offload {
                PlacementTier::GpuOnly
            } else {
                PlacementTier::Hybrid
            };
            let mut c = Self::candidate(
                "User setting", tier, ngl, n_cpu_moe, kv_offload, ctx, &p, ram_budget, 0, true,
                format!("planner bypassed: n_gpu_layers {} n_cpu_moe {} offload_kqv {} -> estimate VRAM {:.2} of usable {:.2} GB",
                    ngl, n_cpu_moe, kv_offload as u8, gib(p.vram), gib(vram_usable)),
            );
            if over_vram(&p) > 0 {
                c.reason = format!("{}; {:.2} GB over usable VRAM (llama.cpp may fail, or Windows moves the rest to shared GPU memory)",
                    c.reason, gib(over_vram(&p)));
            }
            candidates.push(c);
        }

        // STEP 1 - Full GPU: every block + output head + KV + logits in VRAM
        if gpu_allowed {
            let p = place(-1, 0, true);
            candidates.push(Self::candidate(
                "Full GPU", PlacementTier::GpuOnly, -1, 0, true, ctx, &p, ram_budget, over_vram(&p), is_moe,
                format!("weights {:.2} + KV {:.2} + buffers {:.2} = {:.2} GB vs usable VRAM {:.2} GB",
                    gib(w.full_gpu()), gib(kv_total), gib(gpu_logits), gib(p.vram), gib(vram_usable)),
            ));
        }

        // STEP 2 (MoE) - attention/shared weights + KV of every layer on GPU, experts from the last layer back
        // while they fit; experts of the first n_cpu_moe layers stay in RAM (or on disk)
        if gpu_allowed && is_moe {
            let n_cpu_moe = (0..=n_layers).find(|&n| over_vram(&place(-1, n, true)) == 0).unwrap_or(n_layers);
            let mut p = place(-1, n_cpu_moe, true);
            // llama.cpp's MoE cache (crate::moe_cache) when experts stay in RAM and the model fits in RAM;
            // a model read from disk keeps this placement without a cache (run 13: the cache was slower there),
            // unless CLUAIZ_MOE_CACHE=on (a test switch) asks for the cache there too
            let mode = crate::moe_cache::mode();
            let (n_cpu_moe, cache) = match moe_cache_split(n_cpu_moe, n_layers, experts, unified, mode, &place, vram_usable, ram_budget) {
                Some((n, cache, with_cache)) => {
                    p = with_cache;
                    (n, Some(cache))
                }
                None => (n_cpu_moe, None),
            };
            let cache_note = cache.map(|c| format!(", MoE cache {:.2} GB in VRAM (CPU share {})", gib(c.bytes), c.cpu_share)).unwrap_or_default();
            let mut c = Self::candidate(
                "MoE split", PlacementTier::MoeSplit, -1, n_cpu_moe as u32, true, ctx, &p, ram_budget, over_vram(&p), true,
                format!("attention/shared {:.2} + KV {:.2} + buffers {:.2} on GPU, experts of {} / {} layers in RAM ({:.2} GB){}",
                    gib(w.full_gpu() - experts), gib(kv_total), gib(gpu_logits_for(w.full_gpu() - experts)), n_cpu_moe, n_layers,
                    gib(w.layer_experts[..n_cpu_moe].iter().sum()), cache_note),
            );
            if let Some(cache) = cache {
                c.moe_cache_bytes = cache.bytes;
                c.moe_cache_cpu_share = cache.cpu_share;
            }
            candidates.push(c);
        }

        // STEP 2b (MoE) - when the attention of every layer does not fit: every expert in RAM, output head +
        // attention/shared weights and KV of as many blocks as fit on the GPU, from the last one back
        // (llama.cpp -ngl N -ncmoe all). Listed only when the full MoE split does not fit and a block does.
        if gpu_allowed && is_moe && n_layers > 1 && over_vram(&place(-1, n_layers, true)) > 0 {
            let fitting = (2..=n_layers as i32).rev().find(|&ngl| over_vram(&place(ngl, n_layers, true)) == 0);
            if let Some(ngl) = fitting {
                let p = place(ngl, n_layers, true);
                let blocks = ngl as usize - 1;
                candidates.push(Self::candidate(
                    "MoE partial split", PlacementTier::MoeSplit, ngl, n_layers as u32, true, ctx, &p, ram_budget, 0, true,
                    format!("output head + attention/shared and KV of {} / {} blocks on GPU ({:.2} GB), every expert in RAM ({:.2} GB)",
                        blocks, n_layers, gib(p.vram), gib(experts)),
                ));
            }
        }

        // CTX rule 5B - all weights on GPU, whole KV in RAM (llama.cpp offload_kqv = 0;
        // attention runs on CPU), before any block leaves the GPU. Auto lists it only when the weights fit
        // and the KV is what does not (Auto first tried a smaller KV and context on the GPU).
        let kv_ram = place(-1, 0, false);
        let kv_is_the_miss = over_vram(&kv_ram) == 0 && over_vram(&place(-1, 0, true)) > 0;
        if gpu_allowed && (opt.context_setting() != ContextSetting::Auto || kv_is_the_miss) {
            candidates.push(Self::candidate(
                "Weights GPU, KV RAM", PlacementTier::Hybrid, -1, 0, false, ctx, &kv_ram, ram_budget, over_vram(&kv_ram), false,
                format!("weights {:.2} + buffers {:.2} on GPU, KV {:.2} GB in RAM (attention on CPU, slower as the chat grows)",
                    gib(w.full_gpu()), gib(gpu_logits), gib(kv_total)),
            ));
        }

        // STEP 2 - Split: output head + logits on GPU, then whole blocks (with their KV) from the last one back.
        // At most n_layers - 1 blocks (all blocks is Full GPU). llama.cpp n_gpu_layers = blocks + 1 (output head).
        if gpu_allowed && n_layers > 1 {
            let blocks = (1..n_layers).rev().find(|&b| over_vram(&place(b as i32 + 1, 0, true)) == 0).unwrap_or(1);
            let p = place(blocks as i32 + 1, 0, true);
            candidates.push(Self::candidate(
                "GPU + RAM split", PlacementTier::Hybrid, blocks as i32 + 1, 0, true, ctx, &p, ram_budget,
                over_vram(&p), is_moe,
                format!("{} / {} blocks + output head on GPU ({:.2} GB), {} blocks in RAM",
                    blocks, n_layers, gib(p.vram), n_layers - blocks),
            ));
        }

        // CPU only: all weights + KV + logits in RAM
        let p = place(0, 0, true);
        candidates.push(Self::candidate(
            "CPU only", PlacementTier::CpuOnly, 0, 0, true, ctx, &p, ram_budget, 0, is_moe,
            format!("weights {:.2} + KV {:.2} + logits {:.2} = {:.2} GB in RAM", gib(w.total), gib(kv_total), gib(logits), gib(p.ram)),
        ));

        for c in candidates.iter_mut() {
            use crate::speed::{measured, Signature, Test};
            let signature = Signature::of(c);
            c.measured_tokens_per_s = measured(Test::Generation, geometry, settings, ctx, kv, &signature);
            c.measured_prompt_tokens_per_s = measured(Test::Prompt, geometry, settings, ctx, kv, &signature);
        }

        // A user placement is used as given. Otherwise rule order: first candidate that fits in RAM;
        // STEP 3 - MoE may read experts from disk when nothing fits
        let in_memory = |c: &ExecutionCandidate| c.feasible && c.disk_stream_gb == 0.0;
        let rule = if opt.is_manual_placement() {
            (!candidates.is_empty()).then_some(0)
        } else {
            candidates.iter().position(in_memory)
                .or_else(|| if is_moe { candidates.iter().position(|c| c.feasible) } else { None })
        };
        // STEP 4 - a placement measured faster replaces the rule-order pick (never the user's)
        let mut instead_of: Option<(String, f64)> = None;
        let selected = rule.map(|rule| {
            let pick = if opt.is_manual_placement() || !in_memory(&candidates[rule]) {
                rule
            } else {
                crate::speed::fastest(&candidates, rule, in_memory)
            };
            if pick != rule {
                instead_of = Some((candidates[rule].name.clone(), candidates[rule].measured_tokens_per_s.unwrap_or(0.0)));
            }
            candidates[pick].clone()
        });

        let (selected, explanation) = match selected {
            Some(c) => {
                let why = match &instead_of {
                    Some((rule_name, rule_tps)) => format!(
                        "{}: measured faster ({:.1} t/s vs {:.1} t/s for {}); {}",
                        c.name, c.measured_tokens_per_s.unwrap_or(0.0), rule_tps, rule_name, c.reason),
                    None => format!("{}: {}", c.name, c.reason),
                };
                (c, why)
            }
            None => {
                let closest = candidates.iter()
                    .min_by(|a, b| a.missing_gb.partial_cmp(&b.missing_gb).unwrap_or(std::cmp::Ordering::Equal));
                let mut none = ExecutionCandidate::infeasible(ctx);
                none.reason = match closest {
                    Some(c) => format!(
                        "Does not fit: closest placement ({}) is {:.2} GB short ({}). Free memory, lower the context, or use a smaller quantization.",
                        c.name, c.missing_gb, c.reason),
                    None => "Does not fit and no placement could be evaluated".to_string(),
                };
                let why = none.reason.clone();
                (none, why)
            }
        };

        ExecutionPlan {
            model_id: model_id.to_string(),
            selected_tier: selected.tier,
            n_gpu_layers: selected.n_gpu_layers,
            n_cpu_moe: selected.n_cpu_moe,
            context_window: ctx,
            total_layers: geometry.block_count,
            vram_budget_gb: selected.vram_required_gb,
            ram_budget_gb: selected.ram_required_gb,
            kv_gpu_gb: selected.kv_gpu_gb,
            kv_ram_gb: selected.kv_ram_gb,
            vram_weights_gb: selected.vram_weights_gb,
            ram_weights_gb: selected.ram_weights_gb,
            vram_free_gb: bytes_to_gib(vram_free),
            ram_free_gb: hardware.memory.available_gb,
            disk_stream_gb: selected.disk_stream_gb,
            kv_offload: selected.kv_offload,
            // llama.cpp default unless the user set it
            op_offload: opt.op_offload.unwrap_or(true),
            moe_cache_bytes: selected.moe_cache_bytes,
            moe_cache_cpu_share: selected.moe_cache_cpu_share,
            // added to the chosen placement only: it never takes part in choosing it
            expert_prefetch_bytes: crate::expert_prefetch::bytes(opt.expert_prefetch, selected.moe_cache_bytes),
            kv_cache_type: kv,
            vram_buffer_gb,
            ram_budget_limit_gb: bytes_to_gib(ram_budget),
            context_reason: choice.reason,
            candidates,
            explanation,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn candidate(
        name: &str, tier: PlacementTier, n_gpu_layers: i32, n_cpu_moe: u32, kv_offload: bool, ctx: u32,
        p: &Placement, ram_budget: u64, vram_over: u64, may_stream_from_disk: bool, detail: String,
    ) -> ExecutionCandidate {
        let fits_vram = vram_over == 0;
        let over_ram = p.ram.saturating_sub(ram_budget);
        // STEP 3: dense weights over the RAM ceiling are refused; MoE reads the rest from disk via mmap
        let feasible = fits_vram && (over_ram == 0 || may_stream_from_disk);
        let reason = if !fits_vram {
            format!("does not fit in VRAM: {}", detail)
        } else if over_ram > 0 && may_stream_from_disk {
            format!("{}; {:.2} GB over the RAM limit is read from disk when used (mmap)", detail, gib(over_ram))
        } else if over_ram > 0 {
            format!("{}; needs {:.2} GB more RAM than allowed", detail, gib(over_ram))
        } else {
            detail
        };
        ExecutionCandidate {
            name: name.to_string(),
            tier: if feasible && over_ram > 0 && tier == PlacementTier::CpuOnly { PlacementTier::SsdStreaming } else { tier },
            n_gpu_layers,
            n_cpu_moe,
            context_window: ctx,
            vram_required_gb: gib(p.vram),
            ram_required_gb: gib(p.ram),
            kv_gpu_gb: gib(p.kv_gpu),
            kv_ram_gb: gib(p.kv_ram),
            vram_weights_gb: gib(p.vram_weights),
            ram_weights_gb: gib(p.ram_weights),
            disk_stream_gb: if feasible { gib(over_ram) } else { 0.0 },
            kv_offload,
            moe_cache_bytes: 0,
            moe_cache_cpu_share: 0.0,
            missing_gb: if feasible { 0.0 } else { gib(vram_over) + if may_stream_from_disk { 0.0 } else { gib(over_ram) } },
            feasible,
            reason,
            measured_tokens_per_s: None,
            measured_prompt_tokens_per_s: None,
        }
    }
}

fn gib(bytes: u64) -> f64 {
    bytes_to_gib(bytes)
}

/// How much of the model a placement runs on the GPU, for comparing placements: Full GPU, then the
/// attention of every block (MoE split), then part of it, then whole blocks, then CPU only
fn rank(plan: &ExecutionPlan) -> u8 {
    match plan.selected_tier {
        PlacementTier::GpuOnly => 4,
        PlacementTier::MoeSplit if plan.n_gpu_layers < 0 => 3,
        PlacementTier::MoeSplit => 2,
        PlacementTier::Hybrid => 1,
        _ => 0,
    }
}

/// The MoE split with llama.cpp's MoE cache (`crate::moe_cache`): (n_cpu_moe, cache, placement with
/// the cache counted in VRAM), or None to keep `n_cpu_moe` without a cache. Only when some experts
/// stay in RAM, the GPU has its own memory, and the placement fits in RAM (no expert read from disk);
/// `Mode::On` drops the last condition when the split without a cache already reads from disk (a split
/// that fits in RAM keeps the rules, so `on` never turns a plan in RAM into one read from disk);
/// `Mode::Off` returns None.
/// - The VRAM left with every expert in RAM holds >= `ALL_EXPERTS_IN_RAM_FROM` of the experts: every
///   expert in RAM and that VRAM as cache.
/// - Otherwise: a cache of `BESIDE_LAYERS` of the experts (or that VRAM, when it is less), and the
///   expert layers that still fit in VRAM.
#[allow(clippy::too_many_arguments)]
fn moe_cache_split(
    n_cpu_moe: usize,
    n_layers: usize,
    experts: u64,
    unified: bool,
    mode: crate::moe_cache::Mode,
    place: &impl Fn(i32, usize, bool) -> Placement,
    vram_usable: u64,
    ram_budget: u64,
) -> Option<(usize, crate::moe_cache::MoeCache, Placement)> {
    use crate::moe_cache::{Mode, MoeCache, ALL_EXPERTS_IN_RAM_FROM, BESIDE_LAYERS};
    if n_cpu_moe == 0 || unified || mode == Mode::Off {
        return None;
    }
    let forced_on_disk = mode == Mode::On && place(-1, n_cpu_moe, true).ram > ram_budget;
    let with_cache = |n: usize, cache: MoeCache| {
        let mut p = place(-1, n, true);
        p.vram += cache.bytes;
        (p.vram <= vram_usable && (p.ram <= ram_budget || forced_on_disk)).then_some((n, cache, p))
    };
    let all_in_ram = place(-1, n_layers, true);
    let room = vram_usable.saturating_sub(all_in_ram.vram);
    if room as f64 >= ALL_EXPERTS_IN_RAM_FROM * experts as f64 {
        if let Some(split) = MoeCache::sized(room, experts).and_then(|cache| with_cache(n_layers, cache)) {
            return Some(split);
        }
    }
    let cache = MoeCache::sized(((BESIDE_LAYERS * experts as f64) as u64).min(room), experts)?;
    (n_cpu_moe..=n_layers).find_map(|n| with_cache(n, cache))
}

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
pub(crate) mod matrix;
