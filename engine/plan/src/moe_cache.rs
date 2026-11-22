//! llama.cpp's MoE GPU cache (`--moe-cache-mib`, `--moe-cache-cpu-share`; llama.cpp #29887, #30966,
//! `docs/moe-cache.md`). When the experts of a MoE model stay in RAM, the cache keeps the most used
//! experts in VRAM for generation (batches up to 32 tokens); a share of the misses is computed on
//! the CPU instead of uploaded. The rules below come from run 13 (official llama.cpp on an L4 GPU
//! limited to 8 and 4 GB; `test/cloud/runs/2026-11-15-run13-moe-cache/SUMMARY.md`).
//!
//! `CLUAIZ_MOE_CACHE=off` turns the cache off for every plan (to compare or debug).
//! `CLUAIZ_MOE_CACHE=on` is a test switch: the same split rules also for a split that reads experts
//! from disk, which the rules otherwise leave without a cache (run 13 found the cache slower there).
//! A split that fits in RAM without a cache keeps the rules, so `on` changes only plans read from disk.
//! It exists so that "cache + expert prefetch" (`crate::expert_prefetch`) can be measured on such a
//! model; it is not a speed setting.

/// The MoE split gets the whole VRAM left after attention and KV as cache, with every expert in RAM,
/// when that cache holds at least this share of the experts (run 13c: 2.15-2.45x faster than
/// `--fit` at 8 GB). Below it, the experts that fit stay in VRAM and a smaller cache is added.
pub const ALL_EXPERTS_IN_RAM_FROM: f64 = 0.20;
/// The cache next to expert layers kept in VRAM: this share of the experts (llama.cpp #29887;
/// run 13c: 1.12-1.53x faster than `--fit` at 4 GB)
pub const BESIDE_LAYERS: f64 = 0.10;
/// A cache smaller than this is not used: it would hold too few experts to help
pub const MIN_BYTES: u64 = 256 * 1024 * 1024;
/// llama.cpp sizes the cache in MiB
const MIB: u64 = 1024 * 1024;

/// A cache size and CPU share for llama.cpp's context params
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoeCache {
    pub bytes: u64,
    pub cpu_share: f32,
}

impl MoeCache {
    /// A cache of `bytes` (rounded down to whole MiB) for `experts` bytes of experts in RAM, or None
    /// when it is under [`MIN_BYTES`]
    pub fn sized(bytes: u64, experts: u64) -> Option<Self> {
        let bytes = bytes / MIB * MIB;
        (bytes >= MIN_BYTES && experts > 0).then(|| Self { bytes, cpu_share: cpu_share(bytes, experts) })
    }
}

/// Share of the misses computed on the CPU, from the cache's share of the experts (llama.cpp's
/// tuner `scripts/moe-cache-tune.py` in run 13b, experts mmap'd): a small cache misses often, and the
/// CPU reads the experts in RAM faster than the GPU uploads them; a large cache misses rarely.
pub fn cpu_share(cache_bytes: u64, experts: u64) -> f32 {
    let share = cache_bytes as f64 / experts.max(1) as f64;
    if share <= 0.12 {
        0.95
    } else if share <= 0.25 {
        0.9
    } else if share <= 0.45 {
        0.8
    } else {
        0.0
    }
}

/// When the planner may use the cache (`CLUAIZ_MOE_CACHE`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Not set (or a value other than on/off): the rules of this module, no cache for a model read from disk
    Rules,
    /// `off`: never
    Off,
    /// `on` (test switch): the same rules, also when the split without a cache reads experts from disk
    On,
}

/// The mode from `CLUAIZ_MOE_CACHE`
pub fn mode() -> Mode {
    #[cfg(test)]
    if let Some(forced) = FORCED.with(|f| f.get()) {
        return forced;
    }
    mode_of(std::env::var("CLUAIZ_MOE_CACHE").ok().as_deref())
}

/// [`mode`] for an environment value (None = not set)
pub fn mode_of(env: Option<&str>) -> Mode {
    match env.map(str::to_ascii_lowercase).as_deref() {
        Some("off") => Mode::Off,
        Some("on") => Mode::On,
        _ => Mode::Rules,
    }
}

// Tests set the mode per thread: the environment is process-wide and the tests run in parallel
#[cfg(test)]
thread_local! {
    static FORCED: std::cell::Cell<Option<Mode>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` with [`mode`] returning `forced` on this thread
#[cfg(test)]
pub(crate) fn with_mode<T>(forced: Mode, f: impl FnOnce() -> T) -> T {
    FORCED.with(|c| c.set(Some(forced)));
    let out = f();
    FORCED.with(|c| c.set(None));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn the_share_follows_the_run_13_rule() {
        // run 13c: Qwen3-30B (16.3 GB of experts) at 4 GB 1.6 GB, at 8 GB 5.4 GB; gpt-oss-20b (9.5 GB) at 8 GB 5.6 GB
        assert_eq!(cpu_share(1662 * MIB, 16740 * MIB), 0.95);
        assert_eq!(cpu_share(1588 * MIB, 9682 * MIB), 0.9);
        assert_eq!(cpu_share(5563 * MIB, 16740 * MIB), 0.8);
        assert_eq!(cpu_share(5685 * MIB, 9682 * MIB), 0.0);
    }

    mod planner {
        use super::*;
        use crate::planner::tests::{hw, moe_like, settings};
        use crate::{PlacementTier, Planner};
        use cluaiz_core::{ContextSetting, KvCacheQuantization};

        fn auto() -> cluaiz_core::ActiveUserSettings {
            settings(ContextSetting::Auto, KvCacheQuantization::Auto)
        }

        fn gpu(free_gb: f64, ram_gb: f64) -> cluaiz_hw::SystemInfo {
            let mut h = hw(free_gb, ram_gb);
            h.gpus[0].vram_total_gb = free_gb.max(4.0);
            h
        }

        #[test]
        fn a_gpu_with_room_takes_every_expert_to_ram_and_the_rest_as_cache() {
            // moe_like: 11.7 GB of experts in 30 layers, fits in 16 GB of RAM; 8 GB of VRAM
            let g = moe_like();
            let plan = Planner::create_plan("moe", &g, &gpu(8.0, 16.0), &auto());
            assert_eq!(plan.selected_tier, PlacementTier::MoeSplit, "{}", plan.explanation);
            assert_eq!(plan.n_cpu_moe, 30, "{}", plan.explanation);
            assert!(plan.moe_cache_bytes as f64 >= ALL_EXPERTS_IN_RAM_FROM * g.weights.experts_total() as f64, "{}", plan.explanation);
            assert_eq!(plan.moe_cache_cpu_share, cpu_share(plan.moe_cache_bytes, g.weights.experts_total()));
            assert_eq!(plan.disk_stream_gb, 0.0);
            assert!(plan.explanation.contains("MoE cache"), "{}", plan.explanation);
        }

        #[test]
        fn a_mid_gpu_keeps_expert_layers_beside_a_tenth_as_cache() {
            // 6.2 GB: the VRAM left with every expert in RAM is 10-20% of the experts
            let g = moe_like();
            let plan = Planner::create_plan("moe", &g, &gpu(6.2, 16.0), &auto());
            assert_eq!(plan.selected_tier, PlacementTier::MoeSplit, "{}", plan.explanation);
            assert!(plan.n_cpu_moe < 30, "{}", plan.explanation);
            let tenth = (BESIDE_LAYERS * g.weights.experts_total() as f64) as u64 / MIB * MIB;
            assert_eq!(plan.moe_cache_bytes, tenth, "{}", plan.explanation);
            assert_eq!(plan.moe_cache_cpu_share, 0.95);
            // The cache is counted in the plan's VRAM, which stays within what the GPU has
            assert!(plan.vram_budget_gb + plan.vram_buffer_gb <= 6.2 + 1e-6, "{}", plan.explanation);
        }

        #[test]
        fn a_small_gpu_gives_what_is_left_as_cache() {
            // 4 GB: less than a tenth of the experts is left with every expert in RAM
            let g = moe_like();
            let plan = Planner::create_plan("moe", &g, &gpu(4.0, 16.0), &auto());
            assert_eq!(plan.selected_tier, PlacementTier::MoeSplit, "{}", plan.explanation);
            assert_eq!(plan.n_cpu_moe, 30, "{}", plan.explanation);
            assert!(plan.moe_cache_bytes >= MIN_BYTES, "{}", plan.explanation);
            assert_eq!(plan.moe_cache_cpu_share, 0.95);
            assert!(plan.vram_budget_gb + plan.vram_buffer_gb <= 4.0 + 1e-6, "{}", plan.explanation);
        }

        #[test]
        fn a_model_read_from_disk_gets_no_cache() {
            // The same setup as moe_bigger_than_vram_and_ram_is_allowed_via_disk: the split stays as it was
            let plan = Planner::create_plan("moe", &moe_like(), &hw(2.8, 5.0), &auto());
            assert_eq!(plan.selected_tier, PlacementTier::MoeSplit, "{}", plan.explanation);
            assert!(plan.disk_stream_gb > 0.0);
            assert_eq!(plan.moe_cache_bytes, 0);
        }

        #[test]
        fn forced_on_a_model_read_from_disk_gets_a_cache() {
            // moe_like (11.7 GB of experts) with 5 GB of RAM and 8 GB of VRAM: the rules read experts
            // from disk without a cache; CLUAIZ_MOE_CACHE=on adds one
            let rules = Planner::create_plan("moe", &moe_like(), &gpu(8.0, 5.0), &auto());
            assert!(rules.disk_stream_gb > 0.0 && rules.moe_cache_bytes == 0, "{}", rules.explanation);
            let on = with_mode(Mode::On, || Planner::create_plan("moe", &moe_like(), &gpu(8.0, 5.0), &auto()));
            assert_eq!(on.selected_tier, PlacementTier::MoeSplit, "{}", on.explanation);
            assert!(on.moe_cache_bytes >= MIN_BYTES, "{}", on.explanation);
            assert_eq!(on.moe_cache_cpu_share, cpu_share(on.moe_cache_bytes, moe_like().weights.experts_total()));
            assert!(on.disk_stream_gb > 0.0, "{}", on.explanation);
            // The cache is counted in the VRAM, which stays within what the GPU has
            assert!(on.vram_budget_gb + on.vram_buffer_gb <= 8.0 + 1e-6, "{}", on.explanation);
            // Experts that leave the VRAM for the cache are read from disk too
            assert!(on.disk_stream_gb >= rules.disk_stream_gb, "{} vs {}", on.disk_stream_gb, rules.disk_stream_gb);
            // Off still turns it off
            let off = with_mode(Mode::Off, || Planner::create_plan("moe", &moe_like(), &gpu(8.0, 16.0), &auto()));
            assert_eq!(off.moe_cache_bytes, 0);
        }

        #[test]
        fn forced_on_changes_only_plans_that_read_from_disk() {
            // Every row of the rule table planned again with CLUAIZ_MOE_CACHE=on: a plan that reads
            // nothing from disk keeps its placement; the run 13 rule holds for the default
            let mut forced = 0;
            for row in crate::planner::matrix::all_plans() {
                if row.plan.disk_stream_gb > 0.0 {
                    assert_eq!(row.plan.moe_cache_bytes, 0, "{}: the default gives a disk plan no cache", row.label);
                }
                let on = with_mode(Mode::On, || Planner::create_plan("matrix", &row.geometry, &row.hardware, &row.settings));
                if row.plan.disk_stream_gb == 0.0 {
                    assert_eq!(selected(&on), selected(&row.plan), "{}", row.label);
                } else if on.moe_cache_bytes > 0 {
                    forced += 1;
                    assert!(on.disk_stream_gb > 0.0, "{}", row.label);
                }
            }
            assert!(forced > 0, "the table has disk plans that get a cache when forced on");
        }

        /// The chosen placement as JSON, without the list of candidates and the context note: when
        /// the KV is lowered to read less from disk, the note quotes the GB saved against the plan
        /// with the larger KV, which reads from disk and so gets a cache when forced on
        fn selected(plan: &crate::ExecutionPlan) -> serde_json::Value {
            let mut v = serde_json::to_value(plan).unwrap();
            let o = v.as_object_mut().unwrap();
            o.remove("candidates");
            o.remove("context_reason");
            v
        }

        #[test]
        fn the_users_placement_and_apple_silicon_get_no_cache() {
            let mut manual = auto();
            manual.optimization.n_cpu_moe = Some(30);
            assert_eq!(Planner::create_plan("moe", &moe_like(), &gpu(8.0, 16.0), &manual).moe_cache_bytes, 0);

            let mut mac = gpu(8.0, 16.0);
            mac.memory.is_unified_memory = true;
            let plan = Planner::create_plan("moe", &moe_like(), &mac, &auto());
            assert!(plan.candidates.iter().all(|c| c.moe_cache_bytes == 0), "{}", plan.explanation);
        }

        #[test]
        fn a_model_that_fits_the_gpu_gets_no_cache() {
            let plan = Planner::create_plan("moe", &moe_like(), &gpu(24.0, 16.0), &auto());
            assert_eq!(plan.selected_tier, PlacementTier::GpuOnly, "{}", plan.explanation);
            assert_eq!(plan.moe_cache_bytes, 0);
        }
    }

    #[test]
    fn the_environment_value_picks_the_mode() {
        assert_eq!(mode_of(None), Mode::Rules);
        assert_eq!(mode_of(Some("off")), Mode::Off);
        assert_eq!(mode_of(Some("OFF")), Mode::Off);
        assert_eq!(mode_of(Some("on")), Mode::On);
        assert_eq!(mode_of(Some("On")), Mode::On);
        assert_eq!(mode_of(Some("auto")), Mode::Rules, "an unknown value keeps the rules");
        assert_eq!(mode_of(Some("")), Mode::Rules);
    }

    #[test]
    fn a_tiny_cache_is_not_used() {
        assert_eq!(MoeCache::sized(MIN_BYTES - 1, 10 * GIB), None);
        assert_eq!(MoeCache::sized(GIB, 0), None);
        let c = MoeCache::sized(GIB + 123, 10 * GIB).unwrap();
        assert_eq!(c.bytes, GIB);
        assert_eq!(c.cpu_share, 0.95);
    }
}
