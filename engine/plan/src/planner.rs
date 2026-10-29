//! 🎯 Placement planner: (model geometry, live hardware, settings) -> ExecutionPlan.
//! Follows the rules in resource_negotiator's header: CTX rules, BUFFER rules, STEP 1-3.
//! All byte counts come from the GGUF tensor table (`ModelGeometry::weights`) and llama.cpp's KV math.
//! Speed is not measured here (STEP 4 is not built yet), so candidates are chosen by rule order.

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
/// GPU before it splits the model (owner's rule: speed first, a smaller context before RAM).
pub const AUTO_CONTEXT_MIN: u32 = 2048;
/// BUFFER rule: Cluaiz may use up to this share of total RAM, other apps included (>= 5% always free).
pub const RAM_CEILING_FRACTION: f64 = 0.95;
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
/// higher than what the model took.
pub fn model_buffer_options(geometry: &ModelGeometry, hardware: &SystemInfo, settings: &ActiveUserSettings) -> Vec<f64> {
    match (settings.optimization.custom_vram_buffer_gb, crate::margin::learned(geometry, settings)) {
        (None, Some(gap)) => vec![gap + crate::margin::reserve_gb(hardware)],
        _ => vram_buffer_options(settings),
    }
}

/// Bytes a placement may use of `free` with `buffer_gb` kept back (a negative buffer corrects an
/// estimate that was measured too high)
fn usable_bytes(free: u64, buffer_gb: f64) -> u64 {
    if buffer_gb >= 0.0 {
        free.saturating_sub(gib_to_bytes(buffer_gb))
    } else {
        free.saturating_add(gib_to_bytes(-buffer_gb))
    }
}

/// RAM Cluaiz may use: 95% of total RAM minus what other apps use now, or free RAM minus the
/// user's GB buffer when set (BUFFER rules: "GB values set by the user replace both").
pub fn ram_budget_bytes(hardware: &SystemInfo, settings: &ActiveUserSettings) -> u64 {
    let total = hardware.memory.total_gb;
    let free = hardware.memory.available_gb;
    let budget_gb = match settings.optimization.custom_ram_buffer_gb {
        Some(buffer) => free - buffer,
        None => free - total * (1.0 - RAM_CEILING_FRACTION),
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
            // Only the preferred option (first KV type, first buffer) grows the context beyond the floor.
            // Fallbacks (512 MiB buffer, Q8 KV) exist to reach the floor, so they stop there;
            // otherwise less free memory could give a bigger context than more free memory.
            let options: Vec<(KvCacheQuantization, f64)> = kv_options.iter()
                .flat_map(|&kv| budget.buffers_gb.iter().map(move |&buf| (kv, buf)))
                .collect();
            for (i, &(kv, buf)) in options.iter().enumerate() {
                let ceiling = if i == 0 { native_ctx } else { floor_ctx };
                let ctx = largest_fitting_context(ceiling, |c| fits(c, kv, buf));
                if ctx >= floor_ctx {
                    return ContextChoice {
                        n_ctx: ctx, kv_quant: kv, buffer_gb: buf, fits_on_device: true,
                        reason: if i == 0 {
                            format!("Auto: largest that fits = {} tokens, KV {:?}, buffer {:.2} GB", ctx, kv, buf)
                        } else {
                            let (kv0, buf0) = options[0];
                            format!("Auto: {} tokens with KV {:?} + buffer {:.2} GB (did not fit with KV {:?} + buffer {:.2} GB)",
                                ctx, kv, buf, kv0, buf0)
                        },
                    };
                }
            }
            // CTX rule 3a: below the floor, down to AUTO_CONTEXT_MIN, while the model still runs
            // fully on the GPU (most compact KV first: the point is to stay on the GPU)
            for &buf in &budget.buffers_gb {
                let ctx = largest_fitting_context(floor_ctx, |c| fits(c, most_compact, buf));
                if ctx >= min_ctx {
                    return ContextChoice {
                        n_ctx: ctx, kv_quant: most_compact, buffer_gb: buf, fits_on_device: true,
                        reason: format!("Auto: {} tokens, KV {:?} + buffer {:.2} GB, to stay on the GPU ({} did not fit)",
                            ctx, most_compact, buf, floor_ctx),
                    };
                }
            }
            // The model is split (or on the CPU): the floor, never more (CTX rule 3)
            ContextChoice {
                n_ctx: floor_ctx, kv_quant: most_compact, buffer_gb: first_buffer, fits_on_device: false,
                reason: format!("Auto: {} tokens, KV {:?}; model does not fit on one device, so it is split and the context stays at the floor",
                    floor_ctx, most_compact),
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

/// Bytes of one llama.cpp placement (weights + KV + logits buffer per device).
struct Placement {
    vram: u64,
    ram: u64,
    kv_gpu: u64,
    kv_ram: u64,
}

impl Planner {
    pub fn create_plan(
        model_id: &str,
        geometry: &ModelGeometry,
        hardware: &SystemInfo,
        settings: &ActiveUserSettings,
    ) -> ExecutionPlan {
        let opt = &settings.optimization;
        let ubatch = opt.resolved_ubatch_size();
        let w = &geometry.weights;
        let n_layers = geometry.block_count as usize;
        let logits = geometry.logits_buffer_bytes(ubatch);
        // llama.cpp's dry run of this model minus the estimate (crate::margin), counted with the
        // logits buffer: both are bytes the GPU takes besides the blocks and their KV
        let correction = crate::margin::correction(geometry, settings).unwrap_or(0.0);
        let gpu_logits = if correction >= 0.0 {
            logits + gib_to_bytes(correction)
        } else {
            logits.saturating_sub(gib_to_bytes(-correction))
        };
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
            DeviceBudget { free_bytes, fixed_bytes: gpu_weights + gpu_logits, buffers_gb: model_buffer_options(geometry, hardware, settings) }
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
            let mut p = Placement { vram: 0, ram: w.input_cpu, kv_gpu: 0, kv_ram: 0 };
            if n_gpu_layers != 0 {
                p.vram += head_gpu + gpu_logits;
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
            let p = place(-1, n_cpu_moe, true);
            candidates.push(Self::candidate(
                "MoE split", PlacementTier::MoeSplit, -1, n_cpu_moe as u32, true, ctx, &p, ram_budget, over_vram(&p), true,
                format!("attention/shared {:.2} + KV {:.2} + buffers {:.2} on GPU, experts of {} / {} layers in RAM ({:.2} GB)",
                    gib(w.full_gpu() - experts), gib(kv_total), gib(gpu_logits), n_cpu_moe, n_layers,
                    gib(w.layer_experts[..n_cpu_moe].iter().sum())),
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

        // CTX rule 5B - all weights on GPU, whole KV in RAM (llama.cpp offload_kqv = 0; attention runs on CPU).
        // Only for Custom/Max: Auto already sized the context to the GPU.
        if gpu_allowed && opt.context_setting() != ContextSetting::Auto {
            let p = place(-1, 0, false);
            candidates.push(Self::candidate(
                "Weights GPU, KV RAM", PlacementTier::Hybrid, -1, 0, false, ctx, &p, ram_budget, over_vram(&p), false,
                format!("weights {:.2} + buffers {:.2} on GPU, KV {:.2} GB in RAM (attention on CPU, slower as the chat grows)",
                    gib(w.full_gpu()), gib(gpu_logits), gib(kv_total)),
            ));
        }

        // CPU only: all weights + KV + logits in RAM
        let p = place(0, 0, true);
        candidates.push(Self::candidate(
            "CPU only", PlacementTier::CpuOnly, 0, 0, true, ctx, &p, ram_budget, 0, is_moe,
            format!("weights {:.2} + KV {:.2} + logits {:.2} = {:.2} GB in RAM", gib(w.total), gib(kv_total), gib(logits), gib(p.ram)),
        ));

        // A user placement is used as given. Otherwise rule order: first candidate that fits in RAM;
        // STEP 3 - MoE may read experts from disk when nothing fits
        let selected = if opt.is_manual_placement() {
            candidates.first().cloned()
        } else {
            candidates.iter().find(|c| c.feasible && c.disk_stream_gb == 0.0)
                .or_else(|| if is_moe { candidates.iter().find(|c| c.feasible) } else { None })
                .cloned()
        };

        let (selected, explanation) = match selected {
            Some(c) => {
                let why = format!("{}: {}", c.name, c.reason);
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
            disk_stream_gb: selected.disk_stream_gb,
            kv_offload: selected.kv_offload,
            // llama.cpp default unless the user set it
            op_offload: opt.op_offload.unwrap_or(true),
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
            disk_stream_gb: if feasible { gib(over_ram) } else { 0.0 },
            kv_offload,
            missing_gb: if feasible { 0.0 } else { gib(vram_over) + if may_stream_from_disk { 0.0 } else { gib(over_ram) } },
            feasible,
            reason,
        }
    }
}

fn gib(bytes: u64) -> f64 {
    bytes_to_gib(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cluaiz_models::WeightBytes;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// qwen3 geometry: 36 layers, 8 KV heads, head size 128, vocab 151936, tied output head.
    fn qwen3_like(file_gb: f64) -> ModelGeometry {
        let total = (file_gb * GIB as f64) as u64;
        let embd = 300 * 1024 * 1024;
        let per_layer = (total - embd) / 36;
        ModelGeometry {
            architecture: "qwen3".into(),
            context_length: 40960,
            embedding_length: 2560,
            block_count: 36,
            head_count: 32,
            head_count_kv: 8,
            head_count_kv_per_layer: vec![8; 36],
            key_length: 128,
            value_length: 128,
            key_length_swa: 128,
            value_length_swa: 128,
            file_size_bytes: total,
            vocab_size: 151_936,
            weights: WeightBytes {
                total,
                input_cpu: embd,
                tied_output_copy: embd,
                layer: vec![per_layer; 36],
                layer_experts: vec![0; 36],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn moe_like() -> ModelGeometry {
        let mut g = qwen3_like(1.0);
        g.block_count = 30;
        g.head_count_kv_per_layer = vec![8; 30];
        g.weights.layer = vec![30 * 1024 * 1024; 30];
        g.weights.layer_experts = vec![400 * 1024 * 1024; 30];
        g.weights.total = g.weights.input_cpu + 30 * (30 + 400) * 1024 * 1024;
        g
    }

    fn settings(ctx: ContextSetting, kv: KvCacheQuantization) -> ActiveUserSettings {
        let mut s = ActiveUserSettings::default();
        s.optimization.set_context_setting(ctx);
        s.optimization.kv_cache_quantization = kv;
        s
    }

    fn hw(vram_free: f64, ram_free: f64) -> SystemInfo {
        let mut h = SystemInfo::default();
        h.memory.total_gb = 23.69;
        h.memory.available_gb = ram_free;
        h.gpus.push(cluaiz_hw::GpuSubsystem { vram_total_gb: 4.0, vram_available_gb: vram_free, ..Default::default() });
        h
    }

    fn gpu_budget(g: &ModelGeometry, free_gb: f64, s: &ActiveUserSettings) -> DeviceBudget {
        DeviceBudget {
            free_bytes: (free_gb * GIB as f64) as u64,
            fixed_bytes: g.weights.full_gpu() + g.logits_buffer_bytes(s.optimization.resolved_ubatch_size()),
            buffers_gb: vram_buffer_options(s),
        }
    }

    #[test]
    fn auto_small_model_gets_at_least_4096_in_f16() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(1.08);
        let c = resolve_context(&g, &gpu_budget(&g, 2.95, &s), &s);
        assert!(c.fits_on_device, "{}", c.reason);
        assert_eq!(c.kv_quant, KvCacheQuantization::Kv16);
        assert!(c.n_ctx >= AUTO_CONTEXT_FLOOR, "got {}", c.n_ctx);
    }

    #[test]
    fn auto_never_goes_below_floor() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(2.33);
        let c = resolve_context(&g, &gpu_budget(&g, 2.0, &s), &s);
        assert_eq!(c.n_ctx, AUTO_CONTEXT_FLOOR);
        assert!(!c.fits_on_device);
    }

    #[test]
    fn custom_is_exact_and_max_is_native() {
        let g = qwen3_like(1.08);
        let s = settings(ContextSetting::Custom(8192), KvCacheQuantization::Auto);
        assert_eq!(resolve_context(&g, &gpu_budget(&g, 2.95, &s), &s).n_ctx, 8192);
        let s = settings(ContextSetting::Max, KvCacheQuantization::Auto);
        assert_eq!(resolve_context(&g, &gpu_budget(&g, 2.95, &s), &s).n_ctx, 40960);
    }

    #[test]
    fn user_kv_type_is_never_changed() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Kv16);
        let g = qwen3_like(2.33);
        assert_eq!(resolve_context(&g, &gpu_budget(&g, 2.8, &s), &s).kv_quant, KvCacheQuantization::Kv16);
    }

    #[test]
    fn buffers_follow_the_rule_only() {
        assert_eq!(vram_buffer_options(&ActiveUserSettings::default()), vec![1.0, 0.5]);
        let mut s = ActiveUserSettings::default();
        s.optimization.custom_vram_buffer_gb = Some(0.25);
        assert_eq!(vram_buffer_options(&s), vec![0.25]);
    }

    #[test]
    fn auto_more_free_vram_never_gives_a_worse_plan() {
        // More free VRAM never moves the model off the GPU, and on the GPU never gives less context.
        // (Going from split to GPU may lower the context: the split keeps the floor, the GPU may take
        // 2048-4095 to stay fast.)
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        for file_gb in [0.53, 1.08, 1.88, 2.33] {
            let g = qwen3_like(file_gb);
            let (mut on_gpu, mut prev) = (false, 0);
            let mut free = 1.0;
            while free <= 8.0 {
                let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
                assert!(c.fits_on_device || !on_gpu, "file {} GB: free {:.2} left the GPU", file_gb, free);
                if c.fits_on_device {
                    assert!(!on_gpu || c.n_ctx >= prev, "file {} GB: free {:.2} gave {} < {}", file_gb, free, c.n_ctx, prev);
                    assert!(c.n_ctx >= AUTO_CONTEXT_MIN.min(g.context_length), "{}", c.reason);
                    on_gpu = true;
                    prev = c.n_ctx;
                }
                free += 0.05;
            }
        }
    }

    #[test]
    fn plan_vram_is_weights_plus_kv_plus_logits() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(1.08);
        let plan = Planner::create_plan("m", &g, &hw(2.95, 8.0), &s);
        assert_eq!(plan.selected_tier, PlacementTier::GpuOnly, "{}", plan.explanation);
        let expect = g.weights.full_gpu() + g.kv_cache_bytes(plan.context_window, plan.kv_cache_type, 512) + g.logits_buffer_bytes(512);
        assert!((plan.vram_budget_gb - bytes_to_gib(expect)).abs() < 1e-9);
        assert!(plan.vram_budget_gb + plan.vram_buffer_gb <= 2.95 + 1e-9);
    }

    #[test]
    fn moe_bigger_than_vram_and_ram_is_allowed_via_disk() {
        // 12+ GB of experts, 2.8 GB free VRAM, ~5 GB free RAM: STEP 3 says MoE is allowed
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let plan = Planner::create_plan("moe", &moe_like(), &hw(2.8, 5.0), &s);
        assert!(plan.is_feasible(), "{}", plan.explanation);
        assert_eq!(plan.selected_tier, PlacementTier::MoeSplit, "{}", plan.explanation);
        assert_eq!(plan.n_gpu_layers, -1);
        assert!(plan.n_cpu_moe > 0 && plan.n_cpu_moe <= 30);
        assert!(plan.disk_stream_gb > 0.0);
    }

    #[test]
    fn dense_too_big_is_refused_with_numbers() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let plan = Planner::create_plan("dense", &qwen3_like(20.0), &hw(2.8, 5.0), &s);
        assert!(!plan.is_feasible());
        assert!(plan.explanation.contains("GB short"), "{}", plan.explanation);
    }

    #[test]
    fn split_keeps_kv_with_its_layer() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let plan = Planner::create_plan("m", &qwen3_like(4.5), &hw(2.8, 12.0), &s);
        assert_eq!(plan.selected_tier, PlacementTier::Hybrid, "{}", plan.explanation);
        assert!(plan.n_gpu_layers > 1 && plan.n_gpu_layers <= 37);
        assert!(plan.kv_gpu_gb > 0.0 && plan.kv_ram_gb > 0.0);
    }

    #[test]
    fn user_placement_is_used_exactly_even_when_over_vram() {
        let mut s = settings(ContextSetting::Custom(4096), KvCacheQuantization::Kv8);
        s.optimization.n_gpu_layers = Some(30);
        let plan = Planner::create_plan("m", &qwen3_like(4.5), &hw(2.8, 12.0), &s);
        assert!(plan.is_feasible(), "{}", plan.explanation);
        assert_eq!(plan.n_gpu_layers, 30);
        assert_eq!(plan.block_split(), (29, 7));
        assert!(plan.explanation.contains("over usable VRAM"), "{}", plan.explanation);
    }

    #[test]
    fn user_gpu_layers_are_kept_when_no_gpu_is_detected() {
        // A Mac: Metal is not seen by the probe, the user still asks for every layer on the GPU
        let mut s = settings(ContextSetting::Custom(4096), KvCacheQuantization::Kv8);
        s.optimization.n_gpu_layers = Some(-1);
        let mut mac = hw(0.0, 12.0);
        mac.gpus.clear();
        let plan = Planner::create_plan("m", &qwen3_like(2.3), &mac, &s);
        assert_eq!(plan.n_gpu_layers, -1, "{}", plan.explanation);
        // Only offload_kqv set: nothing asked about layers, so no GPU means none
        s.optimization.n_gpu_layers = None;
        s.optimization.offload_kqv = Some(true);
        assert_eq!(Planner::create_plan("m", &qwen3_like(2.3), &mac, &s).n_gpu_layers, 0);
    }

    #[test]
    fn apple_silicon_puts_the_model_on_the_gpu_and_counts_it_in_ram() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let mut mac = hw(10.0, 8.0);
        mac.memory.total_gb = 16.0;
        mac.memory.is_unified_memory = true;
        mac.gpus[0].vram_total_gb = 10.7;
        let plan = Planner::create_plan("m", &qwen3_like(2.3), &mac, &s);
        assert_eq!(plan.selected_tier, PlacementTier::GpuOnly, "{}", plan.explanation);
        let full = plan.candidates.iter().find(|c| c.name == "Full GPU").unwrap();
        assert!(full.ram_required_gb >= full.vram_required_gb, "{}", full.reason);
        // The same model with almost no free RAM does not fit, though the Metal working set is free
        mac.memory.available_gb = 3.0;
        assert!(!Planner::create_plan("m", &qwen3_like(2.3), &mac, &s).is_feasible());
    }

    #[test]
    fn split_never_duplicates_full_gpu() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let plan = Planner::create_plan("m", &qwen3_like(1.08), &hw(2.95, 8.0), &s);
        let split = plan.candidates.iter().find(|c| c.name == "GPU + RAM split").unwrap();
        assert!((split.n_gpu_layers as u32) < plan.total_layers + 1);
    }

    #[test]
    fn split_model_keeps_the_floor_and_never_grows_its_context_in_ram() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(4.5);
        for ram in [5.0, 8.0, 12.0, 22.0] {
            let plan = Planner::create_plan("m", &g, &hw(2.8, ram), &s);
            assert!(plan.is_feasible(), "{}", plan.explanation);
            assert_eq!(plan.context_window, AUTO_CONTEXT_FLOOR, "ram {ram}: {}", plan.context_reason);
        }
        // Custom stays exact
        let custom = settings(ContextSetting::Custom(8192), KvCacheQuantization::Auto);
        assert_eq!(Planner::create_plan("m", &g, &hw(2.8, 12.0), &custom).context_window, 8192);
    }

    #[test]
    fn auto_takes_a_smaller_context_on_the_gpu_before_splitting() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(2.33);
        let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
        let kv = |ctx| g.kv_cache_bytes(ctx, KvCacheQuantization::Kv8, 512);
        // Room for 3072 tokens of Q8 KV with the smaller buffer, not for 4096
        let free = bytes_to_gib(fixed + kv(3072)) + MIN_SAFE_VRAM_BUFFER_GB + 0.001;
        let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
        assert!(c.fits_on_device, "{}", c.reason);
        assert!((AUTO_CONTEXT_MIN..AUTO_CONTEXT_FLOOR).contains(&c.n_ctx), "{}", c.reason);
        let plan = Planner::create_plan("m", &g, &hw(free, 8.0), &s);
        assert_eq!(plan.selected_tier, PlacementTier::GpuOnly, "{}", plan.explanation);
        // Not even 2048 fits: split at the floor
        let free = bytes_to_gib(fixed + kv(1024)) + MIN_SAFE_VRAM_BUFFER_GB;
        let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
        assert!(!c.fits_on_device && c.n_ctx == AUTO_CONTEXT_FLOOR, "{}", c.reason);
    }

    #[test]
    fn a_measured_model_is_planned_with_its_measurement_not_the_fixed_buffer() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        // A size no other test uses: the measurements are process-wide
        let g = qwen3_like(2.3317);
        let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
        let kv = g.kv_cache_bytes(4096, KvCacheQuantization::Kv8, 512);
        let hardware = hw(bytes_to_gib(fixed + kv) + 0.30, 8.0);
        // Not measured: the 1024 / 512 MiB rule splits it
        let before = Planner::create_plan("m", &g, &hardware, &s);
        assert_ne!(before.selected_tier, PlacementTier::GpuOnly, "{}", before.explanation);
        // Measured: it took 0.18 GB more than the estimate; with the 0.20 GB reserve it runs fully
        // on the GPU (with a context between 2048 and 4096)
        crate::margin::record(&g, &s, 0.18);
        let after = Planner::create_plan("m", &g, &hardware, &s);
        assert_eq!(after.selected_tier, PlacementTier::GpuOnly, "{}", after.explanation);
        assert!((after.vram_buffer_gb - (0.18 + 0.20)).abs() < 1e-9);
        // The user's value still wins
        let mut custom = s.clone();
        custom.optimization.custom_vram_buffer_gb = Some(1.0);
        assert_eq!(Planner::create_plan("m", &g, &hardware, &custom).vram_buffer_gb, 1.0);
        // An estimate measured too high gives room back
        let g2 = qwen3_like(2.3318);
        crate::margin::record(&g2, &s, -0.40);
        let buffer = model_buffer_options(&g2, &hardware, &s)[0];
        assert!((buffer - (-0.40 + 0.20)).abs() < 1e-9);
    }

    #[test]
    fn a_load_that_fell_back_is_not_planned_the_same_way_again() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        // A size no other test uses: the measurements are process-wide
        let g = qwen3_like(2.3319);
        let hardware = hw(3.6, 8.0);
        let first = Planner::create_plan("m", &g, &hardware, &s);
        assert_eq!(first.selected_tier, PlacementTier::GpuOnly, "{}", first.explanation);
        // llama.cpp could not load it there: the host keeps "needed all that was free"
        crate::margin::record(&g, &s, 3.6 - first.vram_budget_gb);
        let next = Planner::create_plan("m", &g, &hardware, &s);
        assert!(next.vram_budget_gb < first.vram_budget_gb, "{}", next.explanation);
        assert!(next.is_feasible(), "{}", next.explanation);
    }

    #[test]
    fn cpu_only_setting_uses_ram() {
        let mut s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        s.optimization.n_gpu_layers = Some(0);
        let plan = Planner::create_plan("m", &qwen3_like(1.08), &hw(2.95, 8.0), &s);
        assert_eq!(plan.selected_tier, PlacementTier::CpuOnly, "{}", plan.explanation);
        assert_eq!(plan.n_gpu_layers, 0);
        assert_eq!(plan.vram_budget_gb, 0.0);
        // Auto does not fill RAM with context; Custom still gets its number
        assert_eq!(plan.context_window, AUTO_CONTEXT_FLOOR, "{}", plan.context_reason);
        s.optimization.set_context_setting(ContextSetting::Custom(16384));
        assert_eq!(Planner::create_plan("m", &qwen3_like(1.08), &hw(2.95, 8.0), &s).context_window, 16384);
    }
}
