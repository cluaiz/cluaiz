//! The llama.cpp fork's MoE expert prefetch (`engine/llama/llama.cpp/CLUAIZ_PATCHES.md`): right before
//! llama.cpp's MoE GPU cache (`crate::moe_cache`) uploads the experts a layer selected, the fork asks
//! the OS to read those experts (and the misses left to the CPU) from the model file in one call, with
//! llama.cpp's own `llama_prefetch`. It runs only in the cache's upload path, so a plan without a MoE
//! cache gets nothing. It is advisory: a failed prefetch logs a warning and the upload reads as before.
//!
//! Auto is off: no run has measured it yet. It changes no placement: the planner only adds the value
//! to the plan it already chose.
//!
//! `CLUAIZ_EXPERT_PREFETCH=on|off` overrides the setting for every plan (to compare or debug).

use cluaiz_core::FeatureState;

/// Bytes the fork may prefetch per MoE cache upload for this setting and cache, 0 = off. The cap is
/// the cache size: one upload never fills more than the cache holds, so the prefetch never reads
/// further ahead than the upload that follows it.
pub fn bytes(setting: FeatureState, moe_cache_bytes: u64) -> u64 {
    cap(setting, std::env::var("CLUAIZ_EXPERT_PREFETCH").ok().as_deref(), moe_cache_bytes)
}

/// [`bytes`] with the environment value given (None = not set; a value other than on/off is ignored)
pub fn cap(setting: FeatureState, env: Option<&str>, moe_cache_bytes: u64) -> u64 {
    let state = match env.map(str::to_ascii_lowercase).as_deref() {
        Some("on") => FeatureState::On,
        Some("off") => FeatureState::Off,
        _ => setting,
    };
    match state {
        FeatureState::On => moe_cache_bytes,
        FeatureState::Off | FeatureState::Auto => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn only_on_with_a_moe_cache_prefetches() {
        assert_eq!(cap(FeatureState::On, None, GIB), GIB);
        assert_eq!(cap(FeatureState::On, None, 0), 0, "no MoE cache, no upload path");
        assert_eq!(cap(FeatureState::Auto, None, GIB), 0, "Auto is off until measured");
        assert_eq!(cap(FeatureState::Off, None, GIB), 0);
    }

    #[test]
    fn the_environment_overrides_the_setting() {
        assert_eq!(cap(FeatureState::Off, Some("on"), GIB), GIB);
        assert_eq!(cap(FeatureState::Auto, Some("ON"), GIB), GIB);
        assert_eq!(cap(FeatureState::On, Some("off"), GIB), 0);
        assert_eq!(cap(FeatureState::On, Some("maybe"), GIB), GIB, "an unknown value keeps the setting");
        assert_eq!(cap(FeatureState::Auto, Some("on"), 0), 0);
    }

    mod planner {
        use crate::planner::matrix::all_plans;
        use crate::Planner;
        use cluaiz_core::FeatureState;

        /// The plan as JSON without the new field: every placement number, reason and candidate
        fn without_prefetch(plan: &crate::ExecutionPlan) -> serde_json::Value {
            let mut v = serde_json::to_value(plan).unwrap();
            v.as_object_mut().unwrap().remove("expert_prefetch_bytes");
            v
        }

        #[test]
        fn the_setting_changes_no_placement_and_auto_and_off_prefetch_nothing() {
            // Every row of the rule table (models x measurement states x settings x GPU sizes x free RAM)
            // is planned again with each setting value: the plan is the same, only the new field differs
            let rows = all_plans();
            assert!(rows.len() > 500, "{} rows", rows.len());
            let mut with_cache = 0;
            for row in &rows {
                assert_eq!(row.settings.optimization.expert_prefetch, FeatureState::Auto);
                if std::env::var_os("CLUAIZ_EXPERT_PREFETCH").is_none() {
                    assert_eq!(row.plan.expert_prefetch_bytes, 0, "{}: Auto must prefetch nothing", row.label);
                }
                for state in [FeatureState::Off, FeatureState::On] {
                    let mut s = row.settings.clone();
                    s.optimization.expert_prefetch = state;
                    let plan = Planner::create_plan("matrix", &row.geometry, &row.hardware, &s);
                    assert_eq!(without_prefetch(&plan), without_prefetch(&row.plan), "{} ({state:?})", row.label);
                    let expected = super::super::bytes(state, plan.moe_cache_bytes);
                    assert_eq!(plan.expert_prefetch_bytes, expected, "{} ({state:?})", row.label);
                }
                with_cache += (row.plan.moe_cache_bytes > 0) as usize;
            }
            assert!(with_cache > 0, "the table has plans with a MoE cache");
        }
    }
}
