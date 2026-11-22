//! The plan in words, in two sizes built from the same parts:
//! - [`ExecutionPlan::report`]: `cluaiz show`. Per GPU / RAM / SSD: free now, kept free and why, what
//!   Cluaiz may use, what goes there and whether it fits; then a one-line summary, context, KV, the
//!   llama.cpp flags, speed and the other placements.
//! - [`ExecutionPlan::summary`]: a few lines after a load (`run`, chat, `bench`, `/plan`).
//!
//! Every number comes from the plan; nothing is planned again here. Memory is in GB of 1024³ bytes,
//! as the OS reports RAM and VRAM (a 4 GB card shows 4.00).

use crate::types::{ExecutionCandidate, ExecutionPlan, PlacementTier};
use cluaiz_core::{group_digits, ActiveUserSettings, ContextSetting, KvCacheQuantization};
use cluaiz_hw::SystemInfo;
use cluaiz_models::ModelGeometry;

/// Below this, a GB value is rounding noise and is not printed
const SMALL_GB: f64 = 0.005;
/// Width of the label column in the device tables
const LABEL: usize = 58;

/// The shape of a placement, the same for the plan and for each candidate
#[derive(Clone, Copy)]
struct Shape {
    tier: PlacementTier,
    n_gpu_layers: i32,
    n_cpu_moe: u32,
    blocks: u32,
}

impl Shape {
    fn of(c: &ExecutionCandidate, blocks: u32) -> Self {
        Self { tier: c.tier, n_gpu_layers: c.n_gpu_layers, n_cpu_moe: c.n_cpu_moe, blocks: blocks.max(1) }
    }

    /// Blocks on the GPU for llama.cpp's n_gpu_layers (-1 = all, N = output head + N-1 blocks, 0 = none)
    fn gpu_blocks(&self) -> u32 {
        match self.n_gpu_layers {
            g if g < 0 => self.blocks,
            0 => 0,
            g => ((g - 1) as u32).min(self.blocks),
        }
    }

    fn on_gpu(&self) -> bool {
        self.n_gpu_layers != 0
    }

    /// Weights on the GPU, in words
    fn gpu_weights(&self) -> String {
        let (n, gpu) = (self.blocks, self.gpu_blocks());
        let expert_blocks = n.saturating_sub(self.n_cpu_moe);
        match self.tier {
            PlacementTier::MoeSplit if self.n_gpu_layers < 0 && expert_blocks == 0 => format!("attention of all {} blocks, output head", n),
            PlacementTier::MoeSplit if self.n_gpu_layers < 0 => format!("attention of all {} blocks, experts of {}, output head", n, expert_blocks),
            PlacementTier::MoeSplit => format!("attention of {} of {} blocks, output head", gpu, n),
            _ if gpu == n => format!("all {} blocks, output head", n),
            _ => format!("{} of {} blocks, output head", gpu, n),
        }
    }

    /// Weights in RAM, in words
    fn ram_weights(&self) -> String {
        let (n, gpu) = (self.blocks, self.gpu_blocks());
        match self.tier {
            _ if !self.on_gpu() => format!("all {} blocks, input embeddings", n),
            PlacementTier::MoeSplit if self.n_gpu_layers < 0 && self.n_cpu_moe > 0 => format!("experts of {} blocks, input embeddings", self.n_cpu_moe.min(n)),
            PlacementTier::MoeSplit if self.n_gpu_layers > 0 => format!("experts of all {} blocks, attention of {}, input embeddings", n, n - gpu),
            _ if gpu < n => format!("{} blocks, input embeddings", n - gpu),
            _ => "input embeddings".to_string(),
        }
    }
}

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
        self.candidates.iter().min_by(|a, b| a.missing_gb.total_cmp(&b.missing_gb))
    }

    /// VRAM a placement may use: free minus the gap kept free (a negative gap keeps nothing back)
    pub fn usable_vram_gb(&self) -> f64 {
        (self.vram_free_gb - self.vram_buffer_gb.max(0.0)).max(0.0)
    }

    fn shape(&self) -> Shape {
        Shape { tier: self.selected_tier, n_gpu_layers: self.n_gpu_layers, n_cpu_moe: self.n_cpu_moe, blocks: self.total_layers.max(1) }
    }

    fn kv_place(&self) -> &'static str {
        match (self.kv_gpu_gb > SMALL_GB, self.kv_ram_gb > SMALL_GB) {
            (true, true) => "next to its blocks, GPU and RAM",
            (true, false) => "on the GPU",
            _ => "in RAM",
        }
    }

    /// Short plan report: what a terminal shows after a load. The full report is [`Self::report`].
    pub fn summary(&self) -> String {
        if !self.is_feasible() {
            return self.rejected_summary();
        }
        let shape = self.shape();
        // A plan without blocks (a provider's model) has no devices to show
        let planned = self.total_layers > 0;
        let mut lines = vec![format!("placement  {}", self.placement_label())];
        if planned && shape.on_gpu() {
            let kv = if self.kv_gpu_gb > SMALL_GB { ", KV" } else { "" };
            lines.push(format!("GPU        {:.2} GB: {}{}", self.vram_budget_gb, shape.gpu_weights(), kv));
        }
        if planned {
            let kv = if self.kv_ram_gb > SMALL_GB { ", KV" } else { "" };
            lines.push(format!("RAM        {:.2} GB: {}{}", self.ram_budget_gb, shape.ram_weights(), kv));
        }
        if self.disk_stream_gb > SMALL_GB {
            lines.push(format!("SSD        {:.2} GB of experts, read when a token needs them", self.disk_stream_gb));
        }
        if self.moe_cache_bytes > 0 {
            lines.push(format!("MoE cache  {:.2} GB in VRAM, CPU share {}", cluaiz_core::bytes_to_gib(self.moe_cache_bytes), self.moe_cache_cpu_share));
        }
        if self.expert_prefetch_bytes > 0 {
            lines.push(format!("prefetch   experts read ahead of each MoE cache upload, at most {:.2} GB", cluaiz_core::bytes_to_gib(self.expert_prefetch_bytes)));
        }
        lines.push(format!("context    {} tokens | KV {} {}", group_digits(self.context_window as u64), kv_name(self.kv_cache_type).to_uppercase(), self.kv_place()));
        if planned {
            lines.push(format!("llama.cpp  {}", llama_flags(self)));
        }
        lines.push(format!("reason     {}", self.explanation));
        lines.join("\n")
    }

    /// A rejected plan: what the closest placement needs against what may be used, per device
    fn rejected_summary(&self) -> String {
        let Some(c) = self.closest_candidate() else {
            return format!("placement  Rejected\nreason     {}", self.explanation);
        };
        let shape = Shape::of(c, self.total_layers);
        let mut lines = vec![format!("placement  DOES NOT FIT | closest: {}{}", c.name, gpu_blocks_note(&shape))];
        if shape.on_gpu() {
            lines.push(format!("GPU        needs {:.2} GB, may use {:.2} GB: {}", c.vram_required_gb, self.usable_vram_gb(), fit(c.vram_required_gb, self.usable_vram_gb())));
        }
        let ram_verdict = if c.disk_stream_gb > 0.0 { "the rest read from the SSD".to_string() } else { fit(c.ram_required_gb, self.ram_budget_limit_gb) };
        lines.push(format!("RAM        needs {:.2} GB, may use {:.2} GB: {}", c.ram_required_gb, self.ram_budget_limit_gb, ram_verdict));
        lines.push(format!("to run it  {}", to_run(self, c)));
        lines.join("\n")
    }

    /// Full plan report for `cluaiz show`: the PLAN and OTHER WAYS IT COULD RUN sections
    pub fn report(&self, geometry: &ModelGeometry, hardware: &SystemInfo, settings: &ActiveUserSettings) -> String {
        let Some(c) = self.shown_candidate() else {
            return format!("PLAN: {}\n  {}", self.placement_label(), self.explanation);
        };
        let shape = Shape::of(c, self.total_layers);
        let mut out = Vec::new();
        if self.is_feasible() {
            out.push(format!("PLAN: {}", self.placement_label()));
        } else {
            out.push("PLAN: DOES NOT FIT".to_string());
            out.push(format!("  Closest   {}{}", c.name, gpu_blocks_note(&shape)));
        }
        out.push(String::new());
        if geometry.is_moe() && c.tier == PlacementTier::MoeSplit {
            out.extend(layer_split(geometry, &shape));
            out.push(String::new());
        }
        out.extend(self.gpu_section(c, &shape, geometry, hardware, settings));
        out.push(String::new());
        out.extend(self.ram_section(c, &shape, hardware));
        out.push(String::new());
        out.extend(self.ssd_section(c, geometry));
        out.push(String::new());
        if self.is_feasible() {
            out.extend(self.choices(c, &shape, geometry));
        } else {
            out.push(format!("  To run it   {}", to_run(self, c)));
        }
        let others: Vec<_> = self.candidates.iter().filter(|o| !std::ptr::eq(*o, c)).collect();
        if !others.is_empty() {
            out.push(String::new());
            out.push("OTHER WAYS IT COULD RUN".to_string());
            for o in others {
                out.push(format!("  {:<20} {:<4} {}", o.name, if o.feasible { "yes" } else { "no" }, other_line(self, o)));
            }
        }
        out.join("\n")
    }

    fn gpu_section(&self, c: &ExecutionCandidate, shape: &Shape, geometry: &ModelGeometry, hardware: &SystemInfo, settings: &ActiveUserSettings) -> Vec<String> {
        let Some(g) = hardware.gpus.first() else {
            return vec!["  GPU: none detected".to_string()];
        };
        if !shape.on_gpu() {
            return vec![format!("  GPU: {} ({:.2} GB card), not used by this placement", g.name, g.vram_total_gb)];
        }
        let usable = self.usable_vram_gb();
        let mut out = vec![format!("  GPU: {} ({:.2} GB card)", g.name, g.vram_total_gb)];
        out.push(row("Free right now", self.vram_free_gb, ""));
        out.push(row_minus("Kept free for the desktop and other apps", self.vram_buffer_gb.max(0.0)));
        out.push(format!("      {}", crate::margin::describe(geometry, settings, hardware, self.vram_buffer_gb)));
        out.push(row("Cluaiz may use", usable, ""));
        out.push(String::new());
        out.push("    What goes on the GPU".to_string());
        out.push(row(&format!("  Weights: {}", shape.gpu_weights()), c.vram_weights_gb, ""));
        if c.kv_gpu_gb > SMALL_GB {
            out.push(row(&format!("  Chat memory (KV cache) for {} tokens", group_digits(c.context_window as u64)), c.kv_gpu_gb, ""));
        }
        let cache = cluaiz_core::bytes_to_gib(c.moe_cache_bytes);
        if cache > SMALL_GB {
            out.push(row("  MoE cache for the experts in RAM", cache, ""));
        }
        let working = c.vram_required_gb - c.vram_weights_gb - c.kv_gpu_gb - cache;
        if working.abs() > SMALL_GB {
            out.push(row("  Working space llama.cpp needs for its calculations", working, ""));
        }
        out.push(row("  Total", c.vram_required_gb, &left_or_short(usable, c.vram_required_gb)));
        out
    }

    fn ram_section(&self, c: &ExecutionCandidate, shape: &Shape, hardware: &SystemInfo) -> Vec<String> {
        let limit = self.ram_budget_limit_gb;
        let mut out = vec![format!("  RAM: {:.2} GB", hardware.memory.total_gb)];
        out.push(row("Free right now", self.ram_free_gb, ""));
        out.push(row_minus("Kept free for the OS and other apps", (self.ram_free_gb - limit).max(0.0)));
        out.push(row("Cluaiz may use", limit, ""));
        out.push(String::new());
        out.push("    What goes in RAM".to_string());
        out.push(row(&format!("  Weights: {}", shape.ram_weights()), c.ram_weights_gb, ""));
        if c.kv_ram_gb > SMALL_GB {
            out.push(row(&format!("  Chat memory (KV cache) for {} tokens", group_digits(c.context_window as u64)), c.kv_ram_gb, ""));
        }
        let other = c.ram_required_gb - c.ram_weights_gb - c.kv_ram_gb;
        if other > SMALL_GB {
            let what = if hardware.memory.is_unified_memory && !hardware.gpus.is_empty() { "  What the GPU uses (unified memory)" } else { "  Working space (logits)" };
            out.push(row(what, other, ""));
        }
        let over = c.ram_required_gb - limit;
        let verdict = if over > SMALL_GB && c.disk_stream_gb > 0.0 {
            format!("{:.2} GB more than allowed: read from the SSD", over)
        } else {
            left_or_short(limit, c.ram_required_gb)
        };
        out.push(row("  Total", c.ram_required_gb, &verdict));
        out
    }

    fn ssd_section(&self, c: &ExecutionCandidate, geometry: &ModelGeometry) -> Vec<String> {
        if c.disk_stream_gb > SMALL_GB {
            let about = expert_blocks_in(geometry, c.disk_stream_gb)
                .map(|b| format!(", about {} of the {} blocks' experts", b, geometry.block_count))
                .unwrap_or_default();
            return vec![
                format!("  SSD: {:.2} GB of experts stay in the model file and are read when a token needs them{}.", c.disk_stream_gb, about),
                "       The OS decides each moment which experts stay in RAM, so it is not a fixed set of blocks.".to_string(),
            ];
        }
        if !self.is_feasible() && !geometry.is_moe() && c.ram_required_gb > self.ram_budget_limit_gb {
            return vec!["  SSD: not used. A dense model reads all its weights for every token, so reading them from the disk is refused.".to_string()];
        }
        vec!["  SSD: not used".to_string()]
    }

    /// One-line summary, context, KV, flags and speed of a plan that loads
    fn choices(&self, c: &ExecutionCandidate, shape: &Shape, geometry: &ModelGeometry) -> Vec<String> {
        let mut one_line = Vec::new();
        if shape.on_gpu() {
            one_line.push(format!("GPU: {}{}", shape.gpu_weights(), if c.kv_gpu_gb > SMALL_GB { " + KV" } else { "" }));
        }
        let from_ssd = expert_blocks_in(geometry, c.disk_stream_gb)
            .filter(|_| c.disk_stream_gb > SMALL_GB)
            .map(|b| format!(" (about {} blocks' experts from the SSD)", b))
            .unwrap_or_default();
        one_line.push(format!("RAM: {}{}{}", shape.ram_weights(), from_ssd, if c.kv_ram_gb > SMALL_GB { " + KV" } else { "" }));
        let speed = match c.measured_tokens_per_s {
            Some(tps) => format!("generation {:.1} t/s{}, measured by cluaiz bench", tps,
                c.measured_prompt_tokens_per_s.map(|pp| format!(", prompt {:.1} t/s", pp)).unwrap_or_default()),
            None => "not measured (chosen by rule order; `cluaiz bench --placements` measures it)".to_string(),
        };
        vec![
            "  In one line".to_string(),
            format!("    {}", one_line.join("   ")),
            String::new(),
            format!("  Context     {} tokens ({})", group_digits(self.context_window as u64), self.context_reason),
            format!("  KV cache    {}, {}, {:.2} GB", kv_name(self.kv_cache_type).to_uppercase(), self.kv_place(), c.kv_gpu_gb + c.kv_ram_gb),
            format!("  llama.cpp   {}", llama_flags(self)),
            format!("  Speed       {}", speed),
        ]
    }
}

/// THIS MACHINE NOW for `cluaiz show`
pub fn machine(hardware: &SystemInfo) -> Vec<String> {
    let mut out = vec!["THIS MACHINE NOW".to_string()];
    match hardware.gpus.first() {
        Some(g) => out.push(format!("  GPU   {} | {:.2} GB, free {:.2} GB", g.name, g.vram_total_gb, g.vram_available_gb)),
        None => out.push("  GPU   none detected".to_string()),
    }
    let unified = if hardware.memory.is_unified_memory { " (unified memory, shared with the GPU)" } else { "" };
    out.push(format!("  RAM   {:.2} GB, free {:.2} GB{}", hardware.memory.total_gb, hardware.memory.available_gb, unified));
    out.push(format!("  CPU   {} | {} cores, {} threads", hardware.cpu.brand.trim(), hardware.cpu.physical_cores, hardware.cpu.logical_threads));
    out
}

/// YOUR SETTINGS for `cluaiz show`: only the settings that change the plan
pub fn user_settings(settings: &ActiveUserSettings) -> Vec<String> {
    let opt = &settings.optimization;
    let context = match opt.context_setting() {
        ContextSetting::Auto => "Auto".to_string(),
        ContextSetting::Max => "Max (the model's own limit)".to_string(),
        ContextSetting::Custom(n) => format!("{} tokens", group_digits(n as u64)),
    };
    let kv = match opt.kv_cache_quantization {
        KvCacheQuantization::Auto => "Auto (F16 first, then Q8_0)".to_string(),
        kv => kv_name(kv).to_uppercase(),
    };
    let mut out = vec![
        "YOUR SETTINGS (the ones that change the plan)".to_string(),
        format!("  Context          {}", context),
        format!("  KV cache         {}", kv),
        format!("  GPU layers       {}", opt.n_gpu_layers.map_or("Auto".to_string(), |n| n.to_string())),
    ];
    if let Some(n) = opt.n_cpu_moe {
        out.push(format!("  Experts in RAM   first {} blocks", n));
    }
    if let Some(on) = opt.offload_kqv {
        out.push(format!("  KV on the GPU    {}", if on { "yes" } else { "no (KV in RAM)" }));
    }
    if let Some(gb) = opt.custom_vram_buffer_gb {
        out.push(format!("  VRAM kept free   {:.2} GB", gb));
    }
    if let Some(gb) = opt.custom_ram_buffer_gb {
        out.push(format!("  RAM kept free    {:.2} GB", gb));
    }
    out
}

/// "How the blocks are split" for a MoE split: the two parts of a block and where each goes
fn layer_split(geometry: &ModelGeometry, shape: &Shape) -> Vec<String> {
    let gib = cluaiz_core::bytes_to_gib;
    let w = &geometry.weights;
    let n = shape.blocks;
    let where_it_goes = if shape.n_gpu_layers < 0 {
        let gpu_experts = n.saturating_sub(shape.n_cpu_moe);
        let rest = if gpu_experts > 0 { format!(", and the experts of {} on the GPU", gpu_experts) } else { String::new() };
        format!("This plan puts the attention of every block on the GPU, the experts of {} blocks in RAM{}.", shape.n_cpu_moe.min(n), rest)
    } else {
        format!("This plan puts the attention of {} of the {} blocks on the GPU and every expert in RAM.", shape.gpu_blocks(), n)
    };
    let experts = format!("experts    big: {} small networks, {} used per token",
        geometry.expert_count.unwrap_or(0), geometry.expert_used_count.unwrap_or(0));
    vec![
        "  How the blocks of this model are split".to_string(),
        format!("    Each of the {} blocks has two parts:", n),
        format!("      {:<56}{:>6.2} GB for all {} blocks", "attention  small: works out which words matter", gib(w.full_gpu() - w.experts_total()), n),
        format!("      {:<56}{:>6.2} GB per block, {:.2} GB in all", experts, gib(w.experts_total()) / n as f64, gib(w.experts_total())),
        format!("    {}", where_it_goes),
    ]
}

fn row(label: &str, gb: f64, note: &str) -> String {
    let line = format!("    {:<LABEL$}{:>8.2} GB", label, gb);
    if note.is_empty() { line } else { format!("{}   {}", line, note) }
}

fn row_minus(label: &str, gb: f64) -> String {
    format!("    {:<LABEL$}- {:>6.2} GB", label, gb)
}

fn left_or_short(may: f64, need: f64) -> String {
    let left = may - need;
    if left >= -SMALL_GB { format!("fits ({:.2} GB left)", left.max(0.0)) } else { format!("{:.2} GB short", -left) }
}

fn fit(need: f64, may: f64) -> String {
    if need <= may + SMALL_GB { "fits".to_string() } else { format!("{:.2} GB short", need - may) }
}

fn gpu_blocks_note(shape: &Shape) -> String {
    if shape.n_gpu_layers > 0 { format!(", {} of {} blocks on the GPU", shape.gpu_blocks(), shape.blocks) } else { String::new() }
}

/// About how many blocks' experts `gb` is (None for a dense model)
fn expert_blocks_in(geometry: &ModelGeometry, gb: f64) -> Option<u32> {
    let per_block = cluaiz_core::bytes_to_gib(geometry.weights.experts_total()) / geometry.block_count.max(1) as f64;
    (per_block > 0.0).then(|| (gb / per_block).round().max(1.0) as u32)
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
    format!("{}, or use a smaller quantization", ways.join(", "))
}

fn other_line(plan: &ExecutionPlan, o: &ExecutionCandidate) -> String {
    let shape = Shape::of(o, plan.total_layers);
    let mut parts = Vec::new();
    if o.tier == PlacementTier::Hybrid && o.n_gpu_layers > 0 {
        parts.push(format!("{}/{} blocks on GPU", shape.gpu_blocks(), shape.blocks));
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
        parts.push(o.measured_tokens_per_s.map_or("not measured".to_string(), |tps| format!("{:.1} t/s measured", tps)));
    } else {
        if o.vram_required_gb - plan.usable_vram_gb() > SMALL_GB {
            parts.push(format!("needs {:.2} GB VRAM, {:.2} GB may be used", o.vram_required_gb, plan.usable_vram_gb()));
        }
        if o.ram_required_gb - plan.ram_budget_limit_gb > SMALL_GB {
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
        assert!(!summary.contains(" 0.00 GB"), "{summary}");
        assert!(summary.contains("closest:") && summary.contains("GB short") && summary.contains("to run it"), "{summary}");
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
        for part in ["What goes on the GPU", "What goes in RAM", "Cluaiz may use", "Total", "llama.cpp   -ngl", "OTHER WAYS IT COULD RUN"] {
            assert!(report.contains(part), "missing {part}:\n{report}");
        }
        assert_eq!(super::llama_flags(&plan).contains("-ncmoe"), plan.n_cpu_moe > 0);
        let summary = plan.summary();
        assert!(summary.contains("RAM ") && summary.contains("llama.cpp  -ngl"), "{summary}");
    }

    #[test]
    fn the_report_holds_for_every_plan_of_the_rule_table() {
        // Every model, GPU size, RAM state and setting of the rule table: the report and the summary
        // are built, show no broken number, and the summary's GPU and RAM totals are the plan's own
        for row in crate::planner::matrix::all_plans() {
            let plan = &row.plan;
            let report = plan.report(&row.geometry, &row.hardware, &row.settings);
            let summary = plan.summary();
            for text in [&report, &summary] {
                assert!(!text.contains("NaN") && !text.contains("inf GB") && !text.contains("-0.00"), "{}:\n{text}", row.label);
            }
            assert!(report.starts_with("PLAN: "), "{}:\n{report}", row.label);
            if plan.is_feasible() {
                assert!(summary.contains(&format!("RAM        {:.2} GB", plan.ram_budget_gb)), "{}:\n{summary}", row.label);
                if plan.n_gpu_layers != 0 {
                    assert!(summary.contains(&format!("GPU        {:.2} GB", plan.vram_budget_gb)), "{}:\n{summary}", row.label);
                }
            } else {
                assert!(report.contains("To run it") && summary.contains("to run it"), "{}:\n{summary}", row.label);
            }
        }
    }
}
