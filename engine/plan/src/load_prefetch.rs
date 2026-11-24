//! Load-time prefetch of the weights that stay in CPU memory (`engine/llama/llama.cpp/CLUAIZ_PATCHES.md`,
//! patch 3). At load, llama.cpp asks the OS to read the whole model file (Linux `MAP_POPULATE` and
//! `POSIX_MADV_WILLNEED`, Windows `PrefetchVirtualMemory`). When the plan reads experts from the disk
//! (`disk_stream_gb > 0`: the model does not fit in free RAM), most of that read is evicted again before
//! use. With this rule on, the fork leaves the weights mapped in the CPU's default buffer (the MoE experts
//! kept on the CPU) out of that prefetch; they are read on first use. Weights copied to the GPU keep it.
//!
//! It changes no placement and adds no plan field: the runtime asks [`lazy_host_weights`] right before
//! the load, and only for a plan that reads from disk. `CLUAIZ_LAZY_HOST_WEIGHTS=on|off` overrides the rule
//! (to compare or debug); `on` still needs a plan that reads from disk.

use crate::ExecutionPlan;

/// Whether the load of this plan leaves the CPU-mapped weights out of the load-time prefetch
pub fn lazy_host_weights(plan: &ExecutionPlan) -> bool {
    let env = std::env::var("CLUAIZ_LAZY_HOST_WEIGHTS").ok();
    decide(plan.disk_stream_gb, env.as_deref(), cfg!(windows))
}

/// [`lazy_host_weights`] with the environment value and the platform given (None = not set; a value
/// other than on/off is ignored)
pub fn decide(disk_stream_gb: f64, env: Option<&str>, windows: bool) -> bool {
    match env.map(str::to_ascii_lowercase).as_deref() {
        Some("on") => true,
        Some("off") => false,
        _ => auto_on(disk_stream_gb, windows),
    }
}

/// Auto's rule: off everywhere for now. Run 61 (Windows laptop, Gemma 4 26B A4B 14.2 GB, 8-14 GB free RAM,
/// 9 pairs with the same placement): load time 8.0 -> 4.3 s and disk read during the load 2.90 -> 0.80 GB
/// (medians), generation 15.77 against 15.54 t/s (ranges overlap), but in the 2 pairs where free RAM fell to
/// 0 MiB the prompt pass was 7-12x slower (4.53 and 7.63 against 52.90 and 57.18 t/s). Linux is not measured.
/// The arguments are the signals a future rule would use (the plan reads from disk; the platform measured).
pub fn auto_on(disk_stream_gb: f64, windows: bool) -> bool {
    let _ = (disk_stream_gb, windows);
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_its_rule_and_the_environment_overrides_it() {
        for windows in [true, false] {
            for gb in [0.0, 1.59, 20.0] {
                assert_eq!(decide(gb, None, windows), auto_on(gb, windows), "{gb} {windows}");
                assert!(decide(gb, Some("on"), windows), "on wins over Auto");
                assert!(decide(gb, Some("ON"), windows));
                assert!(!decide(gb, Some("off"), windows), "off wins over Auto");
                assert_eq!(decide(gb, Some("maybe"), windows), auto_on(gb, windows), "an unknown value keeps Auto");
            }
            assert!(!auto_on(0.0, windows), "a model that fits in free RAM keeps the whole-file prefetch");
        }
    }
}
