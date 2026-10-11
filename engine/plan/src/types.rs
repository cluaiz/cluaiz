//! 📋 Typed Execution Plan Definitions
//! A plan is a pure decision; every number in it comes from the planner, and the banner only prints them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementTier {
    /// Every block + output head on GPU
    GpuOnly,
    /// MoE: attention/shared weights of every layer on GPU, experts of the first `n_cpu_moe` layers in RAM
    MoeSplit,
    /// Last blocks on GPU, first blocks in RAM (KV next to its layer)
    Hybrid,
    CpuOnly,
    /// CPU only, MoE bigger than the RAM limit: experts read from disk when used (mmap)
    SsdStreaming,
    Infeasible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCandidate {
    pub name: String,
    pub tier: PlacementTier,
    /// Value passed to llama.cpp: -1 = all blocks + output head, N = last N-1 blocks + output head, 0 = CPU
    pub n_gpu_layers: i32,
    /// Experts of the first N blocks stay in RAM (llama.cpp `--n-cpu-moe N`)
    pub n_cpu_moe: u32,
    pub context_window: u32,
    pub vram_required_gb: f64,
    pub ram_required_gb: f64,
    pub kv_gpu_gb: f64,
    pub kv_ram_gb: f64,
    /// GB over the RAM limit that the OS reads from disk when used (MoE only)
    pub disk_stream_gb: f64,
    /// false = KV cache in RAM while weights are on GPU (CTX rule 5B, llama.cpp offload_kqv = 0)
    pub kv_offload: bool,
    /// GB that do not fit (VRAM over + RAM over); 0 when the candidate fits
    pub missing_gb: f64,
    pub feasible: bool,
    pub reason: String,
}

impl ExecutionCandidate {
    pub fn infeasible(ctx: u32) -> Self {
        Self {
            name: "No placement".into(),
            tier: PlacementTier::Infeasible,
            n_gpu_layers: 0,
            n_cpu_moe: 0,
            context_window: ctx,
            vram_required_gb: 0.0,
            ram_required_gb: 0.0,
            kv_gpu_gb: 0.0,
            kv_ram_gb: 0.0,
            disk_stream_gb: 0.0,
            kv_offload: false,
            missing_gb: 0.0,
            feasible: false,
            reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub model_id: String,
    pub selected_tier: PlacementTier,
    /// Exact llama.cpp n_gpu_layers for the selected placement
    pub n_gpu_layers: i32,
    /// Exact llama.cpp n_cpu_moe for the selected placement (0 = no expert override)
    pub n_cpu_moe: u32,
    pub context_window: u32,
    pub total_layers: u32,
    /// GPU bytes of the selected placement: weights + KV on GPU + logits buffer
    pub vram_budget_gb: f64,
    /// RAM bytes of the selected placement: weights + KV in RAM (+ logits on CPU only)
    pub ram_budget_gb: f64,
    pub kv_gpu_gb: f64,
    pub kv_ram_gb: f64,
    /// GB the OS reads from disk when used (MoE over the RAM limit), 0 otherwise
    pub disk_stream_gb: f64,
    /// llama.cpp offload_kqv: true = KV next to its GPU layers, false = whole KV in RAM (CTX rule 5B)
    pub kv_offload: bool,
    /// KV cache type chosen by the CTX rules (Auto tries F16, then Q8_0); the engine uses exactly this.
    pub kv_cache_type: cluaiz_core::KvCacheQuantization,
    /// VRAM kept free by the BUFFER rules for this plan
    pub vram_buffer_gb: f64,
    /// RAM Cluaiz may use (95% ceiling, other apps included, or free RAM minus the user's buffer)
    pub ram_budget_limit_gb: f64,
    /// Why this context size and KV type were chosen.
    pub context_reason: String,
    pub candidates: Vec<ExecutionCandidate>,
    pub explanation: String,
}

impl ExecutionPlan {
    /// Returns true if the plan represents a viable hardware placement
    pub fn is_feasible(&self) -> bool {
        self.selected_tier != PlacementTier::Infeasible
    }

    /// llama.cpp use_mmap: the user's setting, forced on when MoE experts are read from disk (STEP 3).
    pub fn use_mmap(&self, settings: &cluaiz_core::ActiveUserSettings) -> bool {
        self.disk_stream_gb > 0.0 || settings.optimization.resolved_use_mmap()
    }

    /// Blocks on GPU / in RAM for the selected placement, from the llama.cpp n_gpu_layers value.
    pub fn block_split(&self) -> (u32, u32) {
        let gpu = match self.n_gpu_layers {
            n if n < 0 => self.total_layers,
            0 => 0,
            n => ((n - 1) as u32).min(self.total_layers),
        };
        (gpu, self.total_layers - gpu)
    }

    /// Formats the resource execution banner printed once per model load
    pub fn format_telemetry_banner(
        &self,
        geometry: &cluaiz_models::ModelGeometry,
        hardware: &cluaiz_hw::SystemInfo,
        settings: &cluaiz_core::ActiveUserSettings,
    ) -> String {
        let gib = cluaiz_core::bytes_to_gib;
        let w = &geometry.weights;
        let gpu_name = hardware.gpus.first().map(|g| g.name.clone()).unwrap_or_else(|| "none".to_string());
        let total_vram = hardware.gpus.first().map(|g| g.vram_total_gb).unwrap_or(0.0);
        let free_vram = hardware.gpus.first().map(|g| g.vram_available_gb).unwrap_or(0.0);
        let buffer_tag = if settings.optimization.custom_vram_buffer_gb.is_some() {
            "user setting".to_string()
        } else {
            format!("{:.0} MiB rule", self.vram_buffer_gb * 1024.0)
        };
        let (gpu_blocks, ram_blocks) = self.block_split();
        let mode = match self.selected_tier {
            PlacementTier::GpuOnly => "Full GPU",
            PlacementTier::MoeSplit => "MoE split (attention on GPU, experts in RAM)",
            PlacementTier::Hybrid if !self.kv_offload => "Weights on GPU, KV in RAM (attention on CPU)",
            PlacementTier::Hybrid => "GPU + RAM split",
            PlacementTier::CpuOnly => "CPU only",
            PlacementTier::SsdStreaming => "CPU only, experts read from disk",
            PlacementTier::Infeasible => "Rejected",
        };
        let moe_line = if geometry.is_moe() {
            format!("{} experts, {} active | experts {:.2} GB",
                geometry.expert_count.unwrap_or(0), geometry.expert_used_count.unwrap_or(0), gib(w.experts_total()))
        } else {
            "dense".to_string()
        };
        let swa = match geometry.sliding_window {
            Some(win) if !geometry.swa_layers.is_empty() => format!(", sliding window {} on {} layers", win, geometry.swa_layers.iter().filter(|s| **s).count()),
            _ => String::new(),
        };

        let line = "═".repeat(116);
        let mut out = String::new();
        out.push_str(&format!("\n╔{}╗\n║{:^116}║\n╠{}╣\n", line, "CLUAIZ RESOURCE EXECUTION PLAN", line));
        out.push_str("║ [MODEL]\n");
        out.push_str(&format!("║   • Model ID        : {}\n", self.model_id));
        out.push_str(&format!("║   • Architecture    : {} | {} layers | heads {} | KV heads {} | head size K {} / V {}{} | {}\n",
            geometry.architecture, geometry.block_count, geometry.head_count, geometry.head_count_kv,
            geometry.key_length, geometry.value_length, swa, moe_line));
        out.push_str(&format!("║   • Weights         : file {:.2} GB | input embeddings {:.2} GB (always CPU) | output head {:.2} GB | vocab {}\n",
            gib(geometry.file_size_bytes), gib(w.input_cpu), gib(w.output_gpu()), geometry.vocab_size));
        out.push_str(&format!("╠{}╣\n║ [CONTEXT]\n", line));
        out.push_str(&format!("║   • Native limit    : {} tokens\n", geometry.context_length));
        out.push_str(&format!("║   • Active context  : {} tokens, KV {:?} ({})\n", self.context_window, self.kv_cache_type, self.context_reason));
        out.push_str(&format!("╠{}╣\n║ [HARDWARE]\n", line));
        out.push_str(&format!("║   • GPU             : {} | free {:.2} of {:.2} GB | buffer kept free {:.2} GB ({})\n",
            gpu_name, free_vram, total_vram, self.vram_buffer_gb, buffer_tag));
        out.push_str(&format!("║   • RAM             : free {:.2} of {:.2} GB | Cluaiz may use {:.2} GB\n",
            hardware.memory.available_gb, hardware.memory.total_gb, self.ram_budget_limit_gb));
        out.push_str(&format!("╠{}╣\n║ [PLACEMENT]\n", line));
        out.push_str(&format!("║   • Mode            : {}\n", mode));
        out.push_str(&format!("║   • llama.cpp       : n_gpu_layers = {} | n_cpu_moe = {} | n_ctx = {} | KV {:?} | offload_kqv = {}\n",
            self.n_gpu_layers, self.n_cpu_moe, self.context_window, self.kv_cache_type, self.kv_offload as u8));
        out.push_str(&format!("║   • Blocks          : {} on GPU, {} in RAM (of {})\n", gpu_blocks, ram_blocks, self.total_layers));
        out.push_str(&format!("║   • VRAM            : {:.2} GB (weights + KV {:.2} + logits)\n", self.vram_budget_gb, self.kv_gpu_gb));
        out.push_str(&format!("║   • RAM             : {:.2} GB (incl. KV {:.2})\n", self.ram_budget_gb, self.kv_ram_gb));
        if self.disk_stream_gb > 0.0 {
            out.push_str(&format!("║   • Disk (mmap)     : {:.2} GB of experts read from disk when used\n", self.disk_stream_gb));
        }
        out.push_str(&format!("║   • Reason          : {}\n", self.explanation));
        out.push_str(&format!("║   • Speed           : not measured (chosen by rule order)\n"));
        out.push_str(&format!("╠{}╣\n║ [CANDIDATES]\n", line));
        for c in &self.candidates {
            out.push_str(&format!("║   • {:<16} {:<10} VRAM {:>5.2} GB | RAM {:>5.2} GB | {}\n",
                c.name, if c.feasible { "[FITS]" } else { "[NO]" }, c.vram_required_gb, c.ram_required_gb, c.reason));
        }
        out.push_str(&format!("╠{}╣\n║ [SETTINGS]\n", line));
        let (n_threads, n_threads_batch) = settings.optimization.resolved_threads(hardware.cpu.physical_cores);
        out.push_str(&format!("║   • batch {} / ubatch {} | threads {} / batch {} | flash attention {:?} | KV setting {:?} -> used {:?} | mmap {}\n",
            settings.optimization.resolved_batch_size(), settings.optimization.resolved_ubatch_size(), n_threads, n_threads_batch,
            settings.optimization.flash_attention, settings.optimization.kv_cache_quantization, self.kv_cache_type,
            self.use_mmap(settings)));
        out.push_str(&format!("╚{}╝\n", line));
        out
    }

    /// Prints the banner to standard output
    pub fn print_telemetry_banner(
        &self,
        geometry: &cluaiz_models::ModelGeometry,
        hardware: &cluaiz_hw::SystemInfo,
        settings: &cluaiz_core::ActiveUserSettings,
    ) {
        println!("{}", self.format_telemetry_banner(geometry, hardware, settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_split_matches_llama_cpp_n_gpu_layers() {
        let mut plan = ExecutionPlan {
            model_id: "m".into(),
            selected_tier: PlacementTier::Hybrid,
            n_gpu_layers: 17,
            n_cpu_moe: 0,
            context_window: 4096,
            total_layers: 32,
            vram_budget_gb: 0.0,
            ram_budget_gb: 0.0,
            kv_gpu_gb: 0.0,
            kv_ram_gb: 0.0,
            disk_stream_gb: 0.0,
            kv_offload: true,
            kv_cache_type: cluaiz_core::KvCacheQuantization::Kv16,
            vram_buffer_gb: 1.0,
            ram_budget_limit_gb: 10.0,
            context_reason: String::new(),
            candidates: vec![],
            explanation: String::new(),
        };
        // n_gpu_layers 17 = output head + last 16 blocks
        assert_eq!(plan.block_split(), (16, 16));
        plan.n_gpu_layers = -1;
        assert_eq!(plan.block_split(), (32, 0));
        plan.n_gpu_layers = 0;
        assert_eq!(plan.block_split(), (0, 32));
    }
}
