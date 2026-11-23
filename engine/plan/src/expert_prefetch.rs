//! The llama.cpp fork's MoE expert prefetch (`engine/llama/llama.cpp/CLUAIZ_PATCHES.md`): right before
//! llama.cpp's MoE GPU cache (`crate::moe_cache`) uploads the experts a layer selected, the fork asks
//! the OS to read those experts (and the misses left to the CPU) from the model file in one call, with
//! llama.cpp's own `llama_prefetch`. It runs only in the cache's upload path, so a plan without a MoE
//! cache gets nothing. It is advisory: a failed prefetch logs a warning and the upload reads as before.
//!
//! It changes no placement: the planner only adds the value to the plan it already chose. The whole
//! feature on the Rust side is this module, its one call in `planner.rs` and the plan field it fills.
//!
//! Auto ([`auto_on`]; plan `.agent/plans/engine/ssd-streaming/03-prefetch-auto.md`) is on only for the
//! one shape measured faster: a MoE cache of at least [`ALL_EXPERTS_IN_RAM_FROM`] of the experts, on
//! Linux (run 43, L4: cache 24.8% of the experts, 1.27-1.43x over the cache alone). It is off for a small
//! cache (run 44, Windows laptop: cache 2.2-3.8% of the experts, about half the speed of the cache alone
//! in the same rounds) and on every platform not measured yet.
//!
//! `CLUAIZ_EXPERT_PREFETCH=on|off` overrides the setting for every plan (to compare or debug).

use cluaiz_core::FeatureState;

use crate::moe_cache::ALL_EXPERTS_IN_RAM_FROM;

/// Bytes the fork may prefetch per MoE cache upload for this setting, cache and experts, 0 = off. The
/// cap is the cache size: one upload never fills more than the cache holds.
pub fn bytes(setting: FeatureState, moe_cache_bytes: u64, experts_bytes: u64) -> u64 {
    let env = std::env::var("CLUAIZ_EXPERT_PREFETCH").ok();
    cap(setting, env.as_deref(), moe_cache_bytes, experts_bytes, cfg!(target_os = "linux"))
}

/// [`bytes`] with the environment value and the platform given (None = not set; a value other than
/// on/off is ignored)
pub fn cap(setting: FeatureState, env: Option<&str>, moe_cache_bytes: u64, experts_bytes: u64, linux: bool) -> u64 {
    let state = match env.map(str::to_ascii_lowercase).as_deref() {
        Some("on") => FeatureState::On,
        Some("off") => FeatureState::Off,
        _ => setting,
    };
    let on = match state {
        FeatureState::On => true,
        FeatureState::Off => false,
        FeatureState::Auto => auto_on(moe_cache_bytes, experts_bytes, linux),
    };
    if on { moe_cache_bytes } else { 0 }
}

/// Auto's rule. The cache share is the planner's own threshold for "every expert in RAM, the whole VRAM
/// left as cache" (`crate::moe_cache`), the only branch run 43 measured; a smaller cache comes from the
/// other branch (at most `BESIDE_LAYERS` = 10% of the experts). A smaller cache misses more (run 44: hit
/// rate 24-39% against 65-81% in run 43), so the prefetch asks more per upload (15.0-18.6 MiB against 4.8 MiB
/// per call) for experts the CPU reads right after. Linux only: the one good run used Linux `madvise`;
/// Windows was measured only with a small cache, macOS not at all.
pub fn auto_on(moe_cache_bytes: u64, experts_bytes: u64, linux: bool) -> bool {
    linux
        && moe_cache_bytes > 0
        && experts_bytes > 0
        && moe_cache_bytes as f64 >= ALL_EXPERTS_IN_RAM_FROM * experts_bytes as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    #[test]
    fn only_on_with_a_moe_cache_prefetches() {
        assert_eq!(cap(FeatureState::On, None, GIB, 10 * GIB, false), GIB);
        assert_eq!(cap(FeatureState::On, None, 0, 10 * GIB, true), 0, "no MoE cache, no upload path");
        assert_eq!(cap(FeatureState::Auto, None, 0, 10 * GIB, true), 0);
        assert_eq!(cap(FeatureState::Off, None, 5 * GIB, 10 * GIB, true), 0, "Off is off for every shape");
        assert_eq!(cap(FeatureState::Auto, None, GIB, 0, true), 0, "a dense model has no experts");
    }

    #[test]
    fn auto_follows_the_measured_shapes() {
        // run 43 (L4, Linux): Qwen3.6-35B-A3B, cache 3602.69 MiB for 14514.00 MiB of host experts: faster
        let (cache, experts) = (3602 * MIB, 14514 * MIB);
        assert_eq!(cap(FeatureState::Auto, None, cache, experts, true), cache, "run 43 shape");
        assert_eq!(cap(FeatureState::Auto, None, cache, experts, false), 0, "not measured off Linux");
        // run 44 (laptop, Windows): Gemma 4 26B A4B, 11797.50 MiB of host experts, caches 258-452 MiB:
        // slower. Off on Linux too: the cache share decides, not only the platform
        for cache_mib in [258, 273, 402, 451] {
            for linux in [true, false] {
                assert_eq!(cap(FeatureState::Auto, None, cache_mib * MIB, 11797 * MIB, linux), 0, "run 44 shape {cache_mib} MiB");
            }
        }
        // run 42 (laptop, model fits in RAM): the planner gave no cache, nothing to prefetch
        assert_eq!(cap(FeatureState::Auto, None, 0, 11797 * MIB, false), 0, "run 42 shape");
        // the edge: the planner's share is on, one MiB under it is off
        let edge = (ALL_EXPERTS_IN_RAM_FROM * (10 * GIB) as f64).ceil() as u64;
        assert!(auto_on(edge, 10 * GIB, true));
        assert!(!auto_on(edge - MIB, 10 * GIB, true));
    }

    #[test]
    fn the_environment_overrides_the_setting() {
        assert_eq!(cap(FeatureState::Off, Some("on"), GIB, 10 * GIB, false), GIB);
        assert_eq!(cap(FeatureState::Auto, Some("ON"), GIB, 10 * GIB, false), GIB, "on wins over Auto's off");
        assert_eq!(cap(FeatureState::On, Some("off"), GIB, 10 * GIB, true), 0);
        assert_eq!(cap(FeatureState::Auto, Some("off"), 5 * GIB, 10 * GIB, true), 0, "off wins over Auto's on");
        assert_eq!(cap(FeatureState::On, Some("maybe"), GIB, 10 * GIB, false), GIB, "an unknown value keeps the setting");
        assert_eq!(cap(FeatureState::Auto, Some("on"), 0, 10 * GIB, true), 0);
    }

    mod planner {
        use crate::planner::matrix::all_plans;
        use crate::Planner;
        use cluaiz_core::FeatureState;

        /// The plan as JSON without the prefetch field: every placement number, reason and candidate
        fn without_prefetch(plan: &crate::ExecutionPlan) -> serde_json::Value {
            let mut v = serde_json::to_value(plan).unwrap();
            v.as_object_mut().unwrap().remove("expert_prefetch_bytes");
            v
        }

        #[test]
        fn the_setting_changes_no_placement_and_auto_follows_its_rule() {
            // Every row of the rule table (models x measurement states x settings x GPU sizes x free RAM)
            // is planned again with each setting value: the plan is the same, only the prefetch field differs
            let rows = all_plans();
            assert!(rows.len() > 500, "{} rows", rows.len());
            let (mut with_cache, mut big) = (0, 0);
            for row in &rows {
                assert_eq!(row.settings.optimization.expert_prefetch, FeatureState::Auto);
                let experts = row.geometry.weights.experts_total();
                if std::env::var_os("CLUAIZ_EXPERT_PREFETCH").is_none() {
                    let auto = super::super::auto_on(row.plan.moe_cache_bytes, experts, cfg!(target_os = "linux"));
                    let expected = if auto { row.plan.moe_cache_bytes } else { 0 };
                    assert_eq!(row.plan.expert_prefetch_bytes, expected, "{}", row.label);
                }
                for state in [FeatureState::Off, FeatureState::On] {
                    let mut s = row.settings.clone();
                    s.optimization.expert_prefetch = state;
                    let plan = Planner::create_plan("matrix", &row.geometry, &row.hardware, &s);
                    assert_eq!(without_prefetch(&plan), without_prefetch(&row.plan), "{} ({state:?})", row.label);
                    let expected = super::super::bytes(state, plan.moe_cache_bytes, experts);
                    assert_eq!(plan.expert_prefetch_bytes, expected, "{} ({state:?})", row.label);
                }
                with_cache += (row.plan.moe_cache_bytes > 0) as usize;
                big += super::super::auto_on(row.plan.moe_cache_bytes, experts, true) as usize;
            }
            // the table has plans on both sides of Auto's cache share, so the rule is checked on real plans
            assert!(big > 0 && big < with_cache, "{big} of {with_cache} plans with a MoE cache have a big cache");
        }
    }
}
