//! The plan as `cluaiz show` prints it: per device, what is free, what is kept free, what Cluaiz may
//! use, what goes there and whether it fits; then the SSD, a one-line summary, the llama.cpp flags and
//! the other placements. Every number comes from the plan; nothing is computed again here.
//!
//! Memory is in GB of 1024³ bytes, as the OS reports RAM and VRAM (a 4 GB card shows 4.00).

use crate::types::{ExecutionCandidate, ExecutionPlan, PlacementTier};
use cluaiz_core::KvCacheQuantization;

/// Below this, a GB value is printed as nothing (rounding noise)
const SMALL_GB: f64 = 0.005;

impl ExecutionPlan {
    /// The candidate the plan loads, or for a rejected plan the one closest to fitting
    pub fn shown_candidate(&self) -> Option<&ExecutionCandidate> {
        if self.is_feasible() {
            return self.candidates.iter().find(|c| c.is_placement_of(self));
        }
        self.closest_candidate()
    }

    /// The placement that misses the fewest GB (what a rejected plan reports)
    pub fn closest_candidate(&self) -> Option<&ExecutionCandidate> {
        self.candidates.iter().min_by(|a, b| a.missing_gb.partial_cmp(&b.missing_gb).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// VRAM a placement may use: free minus the gap kept free (a negative gap keeps nothing back)
    pub fn usable_vram_gb(&self) -> f64 {
        (self.vram_free_gb - self.vram_buffer_gb.max(0.0)).max(0.0)
    }

    /// Full plan report for `cluaiz show`
    pub fn report(
        &self,
        geometry: &cluaiz_models::ModelGeometry,
        hardware: &cluaiz_hw::SystemInfo,
        settings: &cluaiz_core::ActiveUserSettings,
    ) -> String {
        let mut out = Vec::new();
        let Some(c) = self.shown_candidate() else {
            return format!("PLAN  {}\n  {}", self.placement_label(), self.explanation);
        };
        let n = self.total_layers.max(1);
        let (gpu_blocks, _) = blocks(c.n_gpu_layers, n);
        let gpu = hardware.gpus.first();

        if self.is_feasible() {
            out.push(format!("PLAN  {}", self.placement_label()));
        } else {
            out.push("PLAN  DOES NOT FIT".to_string());
            out.push(format!("  Closest   {}{}", c.name, if c.n_gpu_layers > 0 { format!(", {} of {} blocks on the GPU", gpu_blocks, n) } else { String::new() }));
        }
        out.push(String::new());

        // GPU
        match gpu {
            Some(g) if c.n_gpu_layers != 0 => {
                let usable = self.usable_vram_gb();
                out.push(format!("  GPU  {} ({:.2} GB)", g.name, g.vram_total_gb));
                out.push(row("Free right now", self.vram_free_gb, ""));
                let tag = crate::margin::describe(geometry, settings, hardware, self.vram_buffer_gb);
                out.push(row_minus("Kept free for the desktop and other apps", self.vram_buffer_gb.max(0.0), ""));
                out.push(format!("      ({})", tag));
                out.push(row("Cluaiz may use", usable, ""));
                out.push("    What goes on the GPU".to_string());
                out.push(row(&format!("  Weights: {}", gpu_weights(c, n)), c.vram_weights_gb, ""));
                if c.kv_gpu_gb > SMALL_GB {
                    out.push(row(&format!("  Chat memory (KV cache), {} tokens", c.context_window), c.kv_gpu_gb, ""));
                }
                let cache = cluaiz_core::bytes_to_gib(c.moe_cache_bytes);
                if cache > SMALL_GB {
                    out.push(row("  MoE cache for experts in RAM", cache, ""));
                }
                let buffers = c.vram_required_gb - c.vram_weights_gb - c.kv_gpu_gb - cache;
                if buffers.abs() > SMALL_GB {
                    out.push(row("  Working space llama.cpp needs (logits, compute)", buffers, ""));
                }
                let left = usable - c.vram_required_gb;
                let verdict = if left >= -SMALL_GB { format!("fits ({:.2} GB left)", left.max(0.0)) } else { format!("{:.2} GB over", -left) };
                out.push(row("  Total", c.vram_required_gb, &verdict));
            }
            Some(g) => out.push(format!("  GPU  {} ({:.2} GB): not used by this placement", g.name, g.vram_total_gb)),
            None => out.push("  GPU  none detected".to_string()),
        }
        out.push(String::new());

        // RAM
        let limit = self.ram_budget_limit_gb;
        out.push(format!("  RAM  {:.2} GB", hardware.memory.total_gb));
        out.push(row("Free right now", self.ram_free_gb, ""));
        out.push(row_minus("Kept free for the OS and other apps", (self.ram_free_gb - limit).max(0.0), ""));
        out.push(row("Cluaiz may use", limit, ""));
        out.push("    What goes in RAM".to_string());
        out.push(row(&format!("  Weights: {}", ram_weights(c, n)), c.ram_weights_gb, ""));
        if c.kv_ram_gb > SMALL_GB {
            out.push(row(&format!("  Chat memory (KV cache), {} tokens", c.context_window), c.kv_ram_gb, ""));
        }
        let other = c.ram_required_gb - c.ram_weights_gb - c.kv_ram_gb;
        if other > SMALL_GB {
            let what = if hardware.memory.is_unified_memory && gpu.is_some() { "  What the GPU uses (unified memory)" } else { "  Working space (logits)" };
            out.push(row(what, other, ""));
        }
        let over = c.ram_required_gb - limit;
        let verdict = if over <= SMALL_GB {
            format!("fits ({:.2} GB left)", (-over).max(0.0))
        } else if c.disk_stream_gb > 0.0 {
            format!("{:.2} GB more than allowed: read from the SSD", over)
        } else {
            format!("{:.2} GB short", over)
        };
        out.push(row("  Total", c.ram_required_gb, &verdict));
        out.push(String::new());

        // SSD
        let experts_per_block = cluaiz_core::bytes_to_gib(geometry.weights.experts_total()) / n as f64;
        if c.disk_stream_gb > SMALL_GB {
            let about = if experts_per_block > 0.0 { (c.disk_stream_gb / experts_per_block).round().max(1.0) as u32 } else { 0 };
            out.push(format!(
                "  SSD  {:.2} GB of experts stay in the model file and are read when a token needs them, about {} of the {} blocks' experts.",
                c.disk_stream_gb, about, n
            ));
            out.push("       The OS decides each moment which experts stay in RAM, so it is not a fixed set of blocks.".to_string());
        } else if !self.is_feasible() && !geometry.is_moe() && c.ram_required_gb > limit {
            out.push("  SSD  not used: a dense model reads all its weights for every token, so reading them from the disk is refused".to_string());
        } else {
            out.push("  SSD  not used".to_string());
        }
        out.push(String::new());

        if !self.is_feasible() {
            out.push(format!("  To run it  {}", to_run(self, c)));
            out.push(String::new());
        } else {
            out.push(format!("  In one line   {}", one_line(c, n, experts_per_block)));
            let kv_place = match (c.kv_gpu_gb > SMALL_GB, c.kv_ram_gb > SMALL_GB) {
                (true, true) => "next to its blocks, GPU and RAM",
                (true, false) => "on the GPU",
                _ => "in RAM",
            };
            out.push(format!("  Context       {} tokens ({})", self.context_window, self.context_reason));
            out.push(format!("  KV cache      {}, {}, {:.2} GB", kv_name(self.kv_cache_type).to_uppercase(), kv_place, c.kv_gpu_gb + c.kv_ram_gb));
            out.push(format!("  llama.cpp     {}", llama_flags(self)));
            let speed = match c.measured_tokens_per_s {
                Some(tps) => format!("generation {:.1} t/s{}, measured by cluaiz bench", tps,
                    c.measured_prompt_tokens_per_s.map(|pp| format!(", prompt {:.1} t/s", pp)).unwrap_or_default()),
                None => "not measured (chosen by rule order; `cluaiz bench --placements` measures it)".to_string(),
            };
            out.push(format!("  Speed         {}", speed));
            out.push(String::new());
        }

        let others: Vec<_> = self.candidates.iter().filter(|o| !std::ptr::eq(*o, c)).collect();
        if !others.is_empty() {
            out.push("OTHER WAYS IT COULD RUN".to_string());
            for o in others {
                out.push(format!("  {:<20} {:<4} {}", o.name, if o.feasible { "yes" } else { "no" }, other_line(self, o, n)));
            }
        }
        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        out.join("\n")
    }
}

fn row(label: &str, gb: f64, note: &str) -> String {
    let line = format!("    {:<56}{:>8.2} GB", label, gb);
    if note.is_empty() { line } else { format!("{}   {}", line, note) }
}

fn row_minus(label: &str, gb: f64, note: &str) -> String {
    let line = format!("    {:<56}- {:>6.2} GB", label, gb);
    if note.is_empty() { line } else { format!("{}   {}", line, note) }
}

/// Blocks on the GPU / in RAM for a llama.cpp n_gpu_layers value (-1 = all, N = output head + N-1 blocks)
fn blocks(n_gpu_layers: i32, n: u32) -> (u32, u32) {
    let gpu = match n_gpu_layers {
        g if g < 0 => n,
        0 => 0,
        g => ((g - 1) as u32).min(n),
    };
    (gpu, n - gpu)
}

fn gpu_weights(c: &ExecutionCandidate, n: u32) -> String {
    let (gpu, _) = blocks(c.n_gpu_layers, n);
    let moe_blocks = n.saturating_sub(c.n_cpu_moe);
    match c.tier {
        PlacementTier::MoeSplit if c.n_gpu_layers < 0 && moe_blocks == 0 => format!("attention of all {} blocks, output head", n),
        PlacementTier::MoeSplit if c.n_gpu_layers < 0 => format!("attention of all {} blocks, experts of {}, output head", n, moe_blocks),
        PlacementTier::MoeSplit => format!("attention of {} of {} blocks, output head", gpu, n),
        _ if gpu == n => format!("all {} blocks, output head", n),
        _ => format!("{} of {} blocks, output head", gpu, n),
    }
}

fn ram_weights(c: &ExecutionCandidate, n: u32) -> String {
    let (gpu, ram) = blocks(c.n_gpu_layers, n);
    match c.tier {
        _ if c.n_gpu_layers == 0 => format!("all {} blocks, input embeddings", n),
        PlacementTier::MoeSplit if c.n_gpu_layers < 0 && c.n_cpu_moe > 0 => format!("experts of {} blocks, input embeddings", c.n_cpu_moe.min(n)),
        PlacementTier::MoeSplit if c.n_gpu_layers > 0 => format!("experts of all {} blocks, attention of {}, input embeddings", n, n - gpu),
        _ if ram > 0 => format!("{} blocks, input embeddings", ram),
        _ => "input embeddings".to_string(),
    }
}

fn one_line(c: &ExecutionCandidate, n: u32, experts_per_block: f64) -> String {
    let mut parts = Vec::new();
    if c.n_gpu_layers != 0 {
        let kv = if c.kv_gpu_gb > SMALL_GB { " + KV" } else { "" };
        parts.push(format!("GPU: {}{}", gpu_weights(c, n), kv));
    }
    let mut ram = ram_weights(c, n);
    if c.disk_stream_gb > SMALL_GB && experts_per_block > 0.0 {
        let on_disk = (c.disk_stream_gb / experts_per_block).round() as u32;
        ram = format!("{} (about {} blocks' experts read from the SSD)", ram, on_disk.max(1));
    }
    let kv = if c.kv_ram_gb > SMALL_GB { " + KV" } else { "" };
    parts.push(format!("RAM: {}{}", ram, kv));
    parts.join("   ")
}

fn to_run(plan: &ExecutionPlan, c: &ExecutionCandidate) -> String {
    let mut ways = Vec::new();
    let vram_over = c.vram_required_gb - plan.usable_vram_gb();
    if vram_over > SMALL_GB {
        ways.push(format!("free {:.2} GB of VRAM", vram_over));
    }
    let ram_over = c.ram_required_gb - plan.ram_budget_limit_gb;
    if ram_over > SMALL_GB && c.disk_stream_gb == 0.0 {
        ways.push(format!("free {:.2} GB of RAM (close apps)", ram_over));
    }
    ways.push("use a smaller context".to_string());
    ways.push("use a smaller quantization".to_string());
    let last = ways.pop().unwrap_or_default();
    if ways.is_empty() { last } else { format!("{}, or {}", ways.join(", "), last) }
}

fn other_line(plan: &ExecutionPlan, o: &ExecutionCandidate, n: u32) -> String {
    let (gpu, _) = blocks(o.n_gpu_layers, n);
    let mut parts = Vec::new();
    if o.tier == PlacementTier::Hybrid && o.n_gpu_layers > 0 {
        parts.push(format!("{}/{} blocks on GPU", gpu, n));
    }
    if o.feasible {
        parts.push(if o.vram_required_gb > SMALL_GB {
            format!("GPU {:.2} GB, RAM {:.2} GB", o.vram_required_gb, o.ram_required_gb)
        } else {
            format!("RAM {:.2} GB", o.ram_required_gb)
        });
        if o.disk_stream_gb > SMALL_GB {
            parts.push(format!("{:.2} GB from SSD", o.disk_stream_gb));
        }
        parts.push(match o.measured_tokens_per_s {
            Some(tps) => format!("{:.1} t/s measured", tps),
            None => "not measured".to_string(),
        });
    } else {
        let vram_over = o.vram_required_gb - plan.usable_vram_gb();
        if vram_over > SMALL_GB {
            parts.push(format!("needs {:.2} GB VRAM, {:.2} GB may be used", o.vram_required_gb, plan.usable_vram_gb()));
        }
        let ram_over = o.ram_required_gb - plan.ram_budget_limit_gb;
        if ram_over > SMALL_GB {
            parts.push(format!("needs {:.2} GB RAM, {:.2} GB may be used", o.ram_required_gb, plan.ram_budget_limit_gb));
        }
    }
    parts.join(" | ")
}

fn kv_name(kv: KvCacheQuantization) -> &'static str {
    match kv {
        KvCacheQuantization::Kv8 => "q8_0",
        KvCacheQuantization::Kv4 => "q4_0",
        KvCacheQuantization::Kv16 | KvCacheQuantization::Auto => "f16",
    }
}

/// The same placement as llama.cpp command-line flags
pub fn llama_flags(plan: &ExecutionPlan) -> String {
    let kv = kv_name(plan.kv_cache_type);
    let mut flags = format!("-ngl {}", plan.n_gpu_layers);
    if plan.n_cpu_moe > 0 {
        flags.push_str(&format!(" -ncmoe {}", plan.n_cpu_moe));
    }
    flags.push_str(&format!(" -c {} -ctk {} -ctv {}", plan.context_window, kv, kv));
    if !plan.kv_offload && plan.n_gpu_layers != 0 {
        flags.push_str(" -nkvo");
    }
    flags
}

#[cfg(test)]
mod tests {
    use crate::planner::tests::{hw, moe_like, qwen3_like, settings};
    use crate::Planner;
    use cluaiz_core::{ContextSetting, KvCacheQuantization};

    #[test]
    fn every_candidate_splits_into_weights_kv_and_buffers() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        for (g, free) in [(moe_like(), 2.0), (qwen3_like(4.5), 3.0), (qwen3_like(1.0), 8.0)] {
            let plan = Planner::create_plan("m", &g, &hw(free, 16.0), &s);
            for c in &plan.candidates {
                let gpu_rest = c.vram_required_gb - c.vram_weights_gb - c.kv_gpu_gb;
                let ram_rest = c.ram_required_gb - c.ram_weights_gb - c.kv_ram_gb;
                assert!(c.vram_weights_gb >= 0.0 && gpu_rest > -0.01, "{}: {:?}", c.name, c);
                assert!(c.ram_weights_gb > 0.0 && ram_rest > -0.01, "{}: {:?}", c.name, c);
                // Weights on both devices are the whole file's weights (no byte counted twice or lost)
                let weights = c.vram_weights_gb + c.ram_weights_gb;
                assert!(weights >= cluaiz_core::bytes_to_gib(g.weights.total) - 0.01, "{}: {weights}", c.name);
            }
        }
    }

    #[test]
    fn a_rejected_plan_reports_the_closest_placement_not_zeros() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = qwen3_like(40.0);
        let hardware = hw(2.0, 8.0);
        let plan = Planner::create_plan("m", &g, &hardware, &s);
        assert!(!plan.is_feasible());
        let summary = plan.summary();
        assert!(!summary.contains("VRAM 0.00 GB"), "{summary}");
        assert!(summary.contains("closest:") && summary.contains("GB short"), "{summary}");
        let report = plan.report(&g, &hardware, &s);
        assert!(report.contains("DOES NOT FIT") && report.contains("To run it") && report.contains("free "), "{report}");
    }

    #[test]
    fn the_report_shows_what_goes_on_each_device_and_the_llama_cpp_flags() {
        let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
        let g = moe_like();
        let hardware = hw(2.0, 16.0);
        let plan = Planner::create_plan("m", &g, &hardware, &s);
        let report = plan.report(&g, &hardware, &s);
        for part in ["What goes on the GPU", "What goes in RAM", "Cluaiz may use", "Total", "llama.cpp     -ngl", "OTHER WAYS IT COULD RUN"] {
            assert!(report.contains(part), "missing {part}:\n{report}");
        }
        assert_eq!(super::llama_flags(&plan).contains("-ncmoe"), plan.n_cpu_moe > 0);
    }
}
