//! Planner tests (the placement-rule table is in `matrix.rs`). Tests of the rules under the usual
//! VRAM gap call `plan_with_gap`; `create_plan` adds the tight gap on top (`tight_gap`).

use super::*;
use cluaiz_models::WeightBytes;

const GIB: u64 = 1024 * 1024 * 1024;

/// qwen3 geometry: 36 layers, 8 KV heads, head size 128, vocab 151936, tied output head.
pub(crate) fn qwen3_like(file_gb: f64) -> ModelGeometry {
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

pub(crate) fn moe_like() -> ModelGeometry {
    let mut g = qwen3_like(1.0);
    g.block_count = 30;
    g.head_count_kv_per_layer = vec![8; 30];
    g.weights.layer = vec![30 * 1024 * 1024; 30];
    g.weights.layer_experts = vec![400 * 1024 * 1024; 30];
    g.weights.total = g.weights.input_cpu + 30 * (30 + 400) * 1024 * 1024;
    g
}

pub(crate) fn settings(ctx: ContextSetting, kv: KvCacheQuantization) -> ActiveUserSettings {
    let mut s = ActiveUserSettings::default();
    s.optimization.set_context_setting(ctx);
    s.optimization.kv_cache_quantization = kv;
    s
}

pub(crate) fn hw(vram_free: f64, ram_free: f64) -> SystemInfo {
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
fn a_failed_load_records_the_miss_and_plans_a_smaller_placement_once() {
    // A size no other test uses: the learned margin is kept per model
    let g = qwen3_like(2.21);
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let live = hw(3.6, 16.0);
    let first = Planner::create_plan("retry-test", &g, &live, &s);
    assert!(first.n_gpu_layers != 0 && first.is_feasible(), "{}", first.explanation);
    let next = Planner::after_failed_load(&first, &g, 3.6, &live, &s).expect("a smaller placement");
    assert!(
        next.vram_budget_gb < first.vram_budget_gb,
        "{} GB then {} GB", first.vram_budget_gb, next.vram_budget_gb
    );
    // The user's own GPU layers are never second-guessed
    let mut manual = s.clone();
    manual.optimization.n_gpu_layers = Some(20);
    assert!(Planner::after_failed_load(&first, &g, 3.6, &live, &manual).is_none());
    // Nothing was on the GPU: nothing to retry
    let cpu = ExecutionPlan { n_gpu_layers: 0, ..first.clone() };
    assert!(Planner::after_failed_load(&cpu, &g, 3.6, &live, &s).is_none());
}

#[test]
fn auto_small_model_stays_on_the_gpu_with_q8_before_a_smaller_context() {
    // 16384 tokens of F16 KV do not fit; Q8 KV gives more than the floor on the GPU
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(1.08);
    let c = resolve_context(&g, &gpu_budget(&g, 2.95, &s), &s);
    assert!(c.fits_on_device, "{}", c.reason);
    assert_eq!(c.kv_quant, KvCacheQuantization::Kv8);
    assert!(c.n_ctx > AUTO_CONTEXT_FLOOR, "got {}", c.n_ctx);
}

#[test]
fn auto_with_plenty_of_vram_takes_the_target_in_f16_not_the_native_limit() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(1.08);
    let c = resolve_context(&g, &gpu_budget(&g, 12.0, &s), &s);
    assert_eq!((c.n_ctx, c.kv_quant), (AUTO_CONTEXT_TARGET, KvCacheQuantization::Kv16), "{}", c.reason);
}

#[test]
fn auto_uses_q4_kv_only_to_keep_the_floor_on_the_gpu() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(2.33);
    let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
    let kv = |ctx, q| g.kv_cache_bytes(ctx, q, 512);
    // Room for 4096 tokens of Q4 KV, not of Q8
    let free = bytes_to_gib(fixed + kv(4096, KvCacheQuantization::Kv4)) + MIN_SAFE_VRAM_BUFFER_GB + 0.001;
    assert!(fixed + kv(4096, KvCacheQuantization::Kv8) > gib_to_bytes(free - MIN_SAFE_VRAM_BUFFER_GB));
    let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
    assert_eq!((c.n_ctx, c.kv_quant, c.fits_on_device), (AUTO_CONTEXT_FLOOR, KvCacheQuantization::Kv4, true), "{}", c.reason);
}

#[test]
fn auto_never_goes_below_floor() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(2.33);
    let c = resolve_context(&g, &gpu_budget(&g, 2.0, &s), &s);
    assert_eq!((c.n_ctx, c.kv_quant), (AUTO_SPLIT_CONTEXT, KvCacheQuantization::Kv8), "{}", c.reason);
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
fn split_model_gets_the_split_context_and_never_grows_it_in_ram() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(4.5);
    for ram in [5.0, 8.0, 12.0, 22.0] {
        let plan = Planner::plan_with_gap("m", &g, &hw(2.8, ram), &s);
        assert!(plan.is_feasible(), "{}", plan.explanation);
        // Too little RAM for the split context: the floor instead of no plan
        let want = if ram < 6.0 { AUTO_CONTEXT_FLOOR } else { AUTO_SPLIT_CONTEXT };
        assert_eq!(plan.context_window, want, "ram {ram}: {}", plan.context_reason);
    }
    // Custom stays exact
    let custom = settings(ContextSetting::Custom(6144), KvCacheQuantization::Auto);
    assert_eq!(Planner::plan_with_gap("m", &g, &hw(2.8, 12.0), &custom).context_window, 6144);
}

#[test]
fn auto_takes_a_smaller_context_on_the_gpu_before_splitting() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let g = qwen3_like(2.33);
    let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
    let kv = |ctx| g.kv_cache_bytes(ctx, KvCacheQuantization::Kv4, 512);
    // Room for 3072 tokens of Q4 KV with the smaller buffer, not for 4096
    let free = bytes_to_gib(fixed + kv(3072)) + MIN_SAFE_VRAM_BUFFER_GB + 0.001;
    let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
    assert!(c.fits_on_device, "{}", c.reason);
    assert!((AUTO_CONTEXT_MIN..AUTO_CONTEXT_FLOOR).contains(&c.n_ctx), "{}", c.reason);
    let plan = Planner::create_plan("m", &g, &hw(free, 8.0), &s);
    assert_eq!(plan.selected_tier, PlacementTier::GpuOnly, "{}", plan.explanation);
    // Not even 2048 fits: split
    let free = bytes_to_gib(fixed + kv(1024)) + MIN_SAFE_VRAM_BUFFER_GB;
    let c = resolve_context(&g, &gpu_budget(&g, free, &s), &s);
    assert!(!c.fits_on_device && c.n_ctx == AUTO_SPLIT_CONTEXT, "{}", c.reason);
}

#[test]
fn a_measured_model_is_planned_with_its_measurement_not_the_fixed_buffer() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // A size no other test uses: the measurements are process-wide
    let g = qwen3_like(2.3317);
    let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
    // Room for the floor with Q4 KV and 0.40 GB more: not enough with the 512 MiB rule
    let kv = g.kv_cache_bytes(4096, KvCacheQuantization::Kv4, 512);
    let hardware = hw(bytes_to_gib(fixed + kv) + 0.40, 8.0);
    // Not measured: the 1024 / 512 MiB rule splits it
    let before = Planner::plan_with_gap("m", &g, &hardware, &s);
    assert_ne!(before.selected_tier, PlacementTier::GpuOnly, "{}", before.explanation);
    // Measured: it took 0.18 GB more than the estimate; with the 0.20 GB reserve it runs fully
    // on the GPU
    crate::margin::record(&g, &s, 0.18);
    let after = Planner::plan_with_gap("m", &g, &hardware, &s);
    assert_eq!(after.selected_tier, PlacementTier::GpuOnly, "{}", after.explanation);
    assert!((after.vram_buffer_gb - (0.18 + 0.20)).abs() < 1e-9);
    // The user's value still wins
    let mut custom = s.clone();
    custom.optimization.custom_vram_buffer_gb = Some(1.0);
    assert_eq!(Planner::plan_with_gap("m", &g, &hardware, &custom).vram_buffer_gb, 1.0);
    // An estimate measured too high gives room back
    let g2 = qwen3_like(2.3318);
    crate::margin::record(&g2, &s, -0.40);
    let buffer = model_buffer_options(&g2, &hardware, &s)[0];
    assert!((buffer - (-0.40 + 0.20)).abs() < 1e-9);
}

#[test]
fn a_moe_model_not_measured_stays_on_the_gpu_of_a_small_card() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // Sizes no other test uses: the measurements are process-wide
    let mut corrected = moe_like();
    corrected.file_size_bytes = 7_000_001;
    let mut plain = moe_like();
    plain.file_size_bytes = 7_000_002;
    let hardware = hw(1.3, 8.0);
    // Not measured, no dry run: llama.cpp's 1024 / 512 MiB rule as before
    assert_eq!(model_buffer_options(&plain, &hardware, &s), vram_buffer_options(&s));
    // Dry run done: a gap sized by the 4 GB card (0.20 reserve + 0.30 pool)
    crate::margin::record_correction(&corrected, &s, 0.0);
    let gap = model_buffer_options(&corrected, &hardware, &s);
    assert_eq!(gap.len(), 1);
    assert!((gap[0] - 0.50).abs() < 1e-9, "{:?}", gap);
    // Attention of every layer does not fit: part of it goes on the GPU, never CPU only
    let plan = Planner::plan_with_gap("m", &corrected, &hardware, &s);
    assert_ne!(plan.selected_tier, PlacementTier::CpuOnly, "{}", plan.explanation);
    assert!(plan.explanation.starts_with("MoE partial split"), "{}", plan.explanation);
    assert!(plan.n_gpu_layers > 1 && plan.n_cpu_moe == 30, "{}", plan.explanation);
    assert!(plan.vram_budget_gb <= 1.3 - 0.50 + 1e-9, "{}", plan.explanation);
    // A 24 GB card keeps llama.cpp's 1024 MiB
    let mut big = hw(20.0, 64.0);
    big.gpus[0].vram_total_gb = 24.0;
    assert_eq!(crate::margin::card_gap_gb(&big), 1.0);
}

#[test]
fn auto_puts_the_kv_in_ram_only_when_the_weights_fit_and_the_kv_does_not() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // A size no other test uses: the measurements are process-wide. Dry run done: 0.50 GB card gap
    let g = qwen3_like(2.3323);
    crate::margin::record_correction(&g, &s, 0.0);
    let fixed = g.weights.full_gpu() + g.logits_buffer_bytes(512);
    // The weights fit, not even the smallest Auto KV does
    let plan = Planner::plan_with_gap("m", &g, &hw(bytes_to_gib(fixed) + 0.50 + 0.001, 8.0), &s);
    assert!(!plan.kv_offload, "{}", plan.explanation);
    assert_eq!(plan.n_gpu_layers, -1, "{}", plan.explanation);
    // Plenty of VRAM: Full GPU, no KV-in-RAM candidate
    let full = Planner::plan_with_gap("m", &g, &hw(3.9, 8.0), &s);
    assert_eq!(full.selected_tier, PlacementTier::GpuOnly, "{}", full.explanation);
    assert!(full.candidates.iter().all(|c| c.name != "Weights GPU, KV RAM"));
}

#[test]
fn a_gap_from_a_spilled_load_never_plans_more_vram_than_the_gpu_has() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // A size no other test uses: the measurements are process-wide
    let g = qwen3_like(2.3321);
    let hardware = hw(2.0, 8.0);
    // A negative buffer only removes the margin: usable VRAM stays within the free VRAM
    assert_eq!(usable_bytes(gib_to_bytes(2.0), -0.30), gib_to_bytes(2.0));
    assert_eq!(usable_bytes(gib_to_bytes(2.0), 0.50), gib_to_bytes(1.5));
    // A gap larger than the 4 GB GPU (a load that spilled into shared memory) is ignored
    crate::margin::record(&g, &s, -4.96);
    assert_eq!(model_buffer_options(&g, &hardware, &s), vram_buffer_options(&s));
    let plan = Planner::create_plan("m", &g, &hardware, &s);
    assert!(plan.vram_budget_gb <= 2.0, "{}", plan.explanation);
    assert!(crate::margin::describe(&g, &s, &hardware, 1.0).contains("ignored"));
    // A load that left the GPU full, or planned more than was free, is not learned from
    assert!(crate::margin::looks_spilled(3.0, 2.0, 0.5));
    assert!(crate::margin::looks_spilled(1.5, 2.0, 0.05));
    assert!(!crate::margin::looks_spilled(1.5, 2.0, 0.4));
}

#[test]
fn a_placement_measured_faster_replaces_the_rule_order_pick() {
    use crate::speed::{Signature, Test};
    let record = |g: &ModelGeometry, s: &ActiveUserSettings, ctx: u32, kv: KvCacheQuantization, sig: &Signature, tps: f64| {
        crate::speed::record(Test::Generation, g, s, ctx, kv, sig, tps)
    };
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // A size no other test uses: the measurements are process-wide
    let g = qwen3_like(4.5123);
    let live = hw(2.8, 12.0);
    let by_rule = Planner::plan_with_gap("m", &g, &live, &s);
    assert_eq!(by_rule.selected_tier, PlacementTier::Hybrid, "{}", by_rule.explanation);
    let split = by_rule.candidates.iter().find(|c| c.name == "GPU + RAM split").unwrap().clone();
    let cpu = by_rule.candidates.iter().find(|c| c.name == "CPU only").unwrap().clone();
    let (ctx, kv) = (by_rule.context_window, by_rule.kv_cache_type);

    // Only the other placement measured: the rule-order pick is not known to be slower
    record(&g, &s, ctx, kv, &Signature::of(&cpu), 9.0);
    assert_eq!(Planner::plan_with_gap("m", &g, &live, &s).selected_tier, PlacementTier::Hybrid);

    // Both measured, one block away from today's split (free VRAM moved a little): CPU only is
    // faster, so it is loaded
    let near = Signature { n_gpu_layers: split.n_gpu_layers + 1, ..Signature::of(&split) };
    record(&g, &s, ctx, kv, &near, 6.0);
    let measured = Planner::plan_with_gap("m", &g, &live, &s);
    assert_eq!(measured.selected_tier, PlacementTier::CpuOnly, "{}", measured.explanation);
    assert!(measured.explanation.contains("measured faster"), "{}", measured.explanation);
    assert!(measured.format_telemetry_banner(&g, &live, &s).contains("9.0 tokens/s"));

    // Within the 5% run-to-run spread: the rule order stays
    record(&g, &s, ctx, kv, &near, 8.8);
    assert_eq!(Planner::plan_with_gap("m", &g, &live, &s).selected_tier, PlacementTier::Hybrid);

    // Faster at generation but much slower at reading prompts: not worth it
    record(&g, &s, ctx, kv, &near, 6.0);
    crate::speed::record(Test::Prompt, &g, &s, ctx, kv, &near, 180.0);
    crate::speed::record(Test::Prompt, &g, &s, ctx, kv, &Signature::of(&cpu), 48.0);
    assert_eq!(Planner::plan_with_gap("m", &g, &live, &s).selected_tier, PlacementTier::Hybrid);
    crate::speed::record(Test::Prompt, &g, &s, ctx, kv, &Signature::of(&cpu), 175.0);
    assert_eq!(Planner::plan_with_gap("m", &g, &live, &s).selected_tier, PlacementTier::CpuOnly);

    // The user's placement is never replaced
    record(&g, &s, ctx, kv, &near, 1.0);
    let mut manual = s.clone();
    manual.optimization.n_gpu_layers = Some(split.n_gpu_layers);
    manual.optimization.set_context_setting(ContextSetting::Custom(ctx));
    manual.optimization.kv_cache_quantization = kv;
    assert_eq!(Planner::plan_with_gap("m", &g, &live, &manual).n_gpu_layers, split.n_gpu_layers);

    // A different context is a different measurement
    let other = settings(ContextSetting::Custom(ctx * 2), KvCacheQuantization::Auto);
    let plan = Planner::plan_with_gap("m", &g, &live, &other);
    assert!(plan.candidates.iter().all(|c| c.measured_tokens_per_s.is_none()));
}

#[test]
fn a_removed_model_leaves_no_measurements_behind() {
    use crate::speed::{Signature, Test};
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // Sizes no other test uses: the measurements are process-wide
    let (gone, kept) = (qwen3_like(1.7771), qwen3_like(1.7772));
    let sig = Signature { tier: PlacementTier::CpuOnly, n_gpu_layers: 0, n_cpu_moe: 0, kv_offload: true, moe_cache: false };
    for g in [&gone, &kept] {
        crate::margin::record(g, &s, 0.1);
        crate::margin::record_correction(g, &s, -0.05);
        crate::speed::record(Test::Generation, g, &s, 4096, KvCacheQuantization::Kv8, &sig, 10.0);
        crate::speed::record(Test::Prompt, g, &s, 4096, KvCacheQuantization::Kv8, &sig, 50.0);
    }
    crate::margin::set_key("onnx:gone-tts:tts:cuda", 0.3);
    crate::margin::set_key("onnx:gone-tts-large:tts:cuda", 0.3);
    crate::margin::set_key("speech:gone-tts", 0.2);

    assert_eq!(crate::margin::forget_model("gone-tts", &[gone.clone()]), 6);
    assert!(crate::margin::learned(&gone, &s).is_none());
    assert!(crate::margin::correction(&gone, &s).is_none());
    assert!(crate::speed::measured(Test::Generation, &gone, &s, 4096, KvCacheQuantization::Kv8, &sig).is_none());
    // Another model, even one whose id starts the same, keeps its numbers
    assert!(crate::margin::learned(&kept, &s).is_some());
    assert!(crate::speed::measured(Test::Prompt, &kept, &s, 4096, KvCacheQuantization::Kv8, &sig).is_some());
    assert!(crate::margin::learned_key("onnx:gone-tts-large:tts:cuda").is_some());
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

#[test]
fn the_desktop_reserve_is_5_percent_within_its_limits() {
    let gpu = |total: f64| {
        let mut h = hw(1.0, 8.0);
        h.gpus[0].vram_total_gb = total;
        crate::margin::reserve_gb(&h)
    };
    assert_eq!(gpu(2.0), 0.15);
    assert!((gpu(4.0) - 0.20).abs() < 1e-9);
    assert!((gpu(8.0) - 0.40).abs() < 1e-9);
    assert_eq!(gpu(24.0), 1.0);
    assert_eq!(gpu(128.0), 1.0);
    // A GPU measured as used by nothing else keeps the minimum; unknown or shared keeps the share
    let mut h = hw(1.0, 8.0);
    h.gpus[0].vram_total_gb = 24.0;
    h.gpus[0].used_only_by_cluaiz = Some(true);
    assert_eq!(crate::margin::reserve_gb(&h), 0.15);
    h.gpus[0].used_only_by_cluaiz = Some(false);
    assert_eq!(crate::margin::reserve_gb(&h), 1.0);
}

#[test]
fn the_ram_kept_free_is_5_percent_up_to_4_gb() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let budget = |total: f64, free: f64| {
        let mut h = hw(1.0, free);
        h.memory.total_gb = total;
        ram_budget_bytes(&h, &s) as f64 / GIB as f64
    };
    assert!((budget(16.0, 10.0) - 9.2).abs() < 1e-6);
    assert!((budget(64.0, 60.0) - 56.8).abs() < 1e-6);
    assert!((budget(256.0, 250.0) - 246.0).abs() < 1e-6);
}

#[test]
fn a_hybrid_model_counts_kv_only_for_its_attention_layers() {
    // Qwen3.5 4B: 32 layers + 1 next-token layer, every 4th trunk layer is attention
    let mut g = qwen3_like(2.59);
    g.block_count = 33;
    g.head_count_kv_per_layer = vec![4; 33];
    g.key_length = 256;
    g.value_length = 256;
    g.recurrent_layers = (0..33).map(|i| i < 32 && (i + 1) % 4 != 0).collect();
    g.recurrent_state_bytes = (3 * (4096 + 2 * 16 * 128) + 128 * 4096) * 4;
    let attention = g.recurrent_layers.iter().filter(|r| !**r).count() as u64;
    assert_eq!(attention, 9);
    let per_token = 4 * (256 + 256) * 2;
    assert_eq!(
        g.kv_cache_bytes(4096, KvCacheQuantization::Kv16, 512),
        attention * per_token * 4096 + 24 * g.recurrent_state_bytes
    );
    // The state does not grow with the context
    let grows = |q| g.kv_cache_bytes(8192, q, 512) - g.kv_cache_bytes(4096, q, 512);
    assert_eq!(grows(KvCacheQuantization::Kv16), attention * per_token * 4096);
}

#[test]
fn a_tight_gap_is_used_only_when_it_puts_more_of_the_model_on_the_gpu() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // A size no other test uses: the measurements are process-wide. Dry run done: 0.50 GB card gap
    let mut g = moe_like();
    g.file_size_bytes = 7_000_003;
    crate::margin::record_correction(&g, &s, 0.0);
    let mut tightened = 0;
    let mut free = 1.0;
    while free <= 3.5 {
        let hardware = hw(free, 8.0);
        let usual = Planner::plan_with_gap("m", &g, &hardware, &s);
        let plan = Planner::create_plan("m", &g, &hardware, &s);
        // Never less than the minimum reserve kept free, never less on the GPU than the usual gap
        assert!(plan.vram_budget_gb + crate::margin::DEVICE_RESERVE_MIN_GB <= free + 1e-9, "{}", plan.explanation);
        assert!(plan.block_split().0 >= usual.block_split().0, "{}", plan.explanation);
        if plan.vram_buffer_gb < usual.vram_buffer_gb {
            tightened += 1;
            assert!(plan.explanation.contains("kept free instead of"), "{}", plan.explanation);
            assert!(crate::margin::describe(&g, &s, &hardware, plan.vram_buffer_gb).starts_with("tight gap"));
        }
        // A plan that runs fully on the GPU keeps the usual gap
        if usual.selected_tier == PlacementTier::GpuOnly {
            assert_eq!(plan.vram_buffer_gb, usual.vram_buffer_gb);
        }
        free += 0.05;
    }
    assert!(tightened > 0, "the tight gap never changed a plan");
    // The user's buffer is never replaced
    let mut custom = s.clone();
    custom.optimization.custom_vram_buffer_gb = Some(1.0);
    assert_eq!(Planner::create_plan("m", &g, &hw(2.0, 8.0), &custom).vram_buffer_gb, 1.0);
}

#[test]
fn a_moe_split_gets_the_share_of_the_correction_its_gpu_weights_are() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    // Sizes no other test uses: the measurements are process-wide
    let mut with = moe_like();
    with.file_size_bytes = 7_000_004;
    let mut without = moe_like();
    without.file_size_bytes = 7_000_005;
    crate::margin::record_correction(&with, &s, 0.30);
    crate::margin::record_correction(&without, &s, 0.0);
    // Too little VRAM for any expert or a MoE cache: both splits keep every expert in RAM
    let hardware = hw(1.0, 16.0);
    let split = |g: &ModelGeometry| {
        Planner::plan_with_gap("m", g, &hardware, &s).candidates.into_iter().find(|c| c.name == "MoE split").unwrap()
    };
    let full = |g: &ModelGeometry| {
        Planner::plan_with_gap("m", g, &hardware, &s).candidates.into_iter().find(|c| c.name == "Full GPU").unwrap()
    };
    // Full GPU gets the whole correction
    assert!((full(&with).vram_required_gb - full(&without).vram_required_gb - 0.30).abs() < 0.01);
    // The MoE split holds the attention (30 x 30 MiB) and the output head of 30 x 430 MiB + head: a few percent
    let (a, b) = (split(&with), split(&without));
    assert!(a.n_cpu_moe == 30 && b.n_cpu_moe == 30 && a.moe_cache_bytes == 0, "{} | {}", a.reason, b.reason);
    let added = a.vram_required_gb - b.vram_required_gb;
    assert!(added > 0.0 && added < 0.05, "{added}");
}

#[test]
fn a_load_that_left_too_little_vram_steps_down_once_and_never_reloads_the_same_plan() {
    // A size no other test uses: the learned margin is kept per model
    let g = qwen3_like(2.23);
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let planned_on = hw(3.6, 16.0);
    let loaded = Planner::create_plan("step-down-test", &g, &planned_on, &s);
    assert!(loaded.n_gpu_layers != 0 && loaded.is_feasible(), "{}", loaded.explanation);
    // Enough left free: the plan stays
    assert!(matches!(Planner::after_low_vram(&loaded, &g, 0.5, &planned_on, &s), StepDown::Keep));
    // Too little left but nothing learned yet: re-planning gives the same plan, which is never loaded again
    assert!(matches!(Planner::after_low_vram(&loaded, &g, 0.05, &planned_on, &s), StepDown::NoSmaller));
    // The load recorded that the model needed 1.5 GB more than planned (more than any default gap):
    // the next plan is smaller
    crate::margin::record(&g, &s, 1.5);
    match Planner::after_low_vram(&loaded, &g, 0.05, &planned_on, &s) {
        StepDown::Smaller(next) => assert!(next.vram_budget_gb + 0.01 < loaded.vram_budget_gb,
            "{} GB then {} GB", loaded.vram_budget_gb, next.vram_budget_gb),
        other => panic!("expected a smaller plan, got {other:?}"),
    }
    // Nothing on the GPU, or no GPU: nothing to step down
    let cpu = ExecutionPlan { n_gpu_layers: 0, ..loaded.clone() };
    assert!(matches!(Planner::after_low_vram(&cpu, &g, 0.0, &planned_on, &s), StepDown::Keep));
    let mut no_gpu = planned_on.clone();
    no_gpu.gpus.clear();
    assert!(matches!(Planner::after_low_vram(&loaded, &g, 0.0, &no_gpu, &s), StepDown::Keep));
}
