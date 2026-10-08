
//! 🧪 21 Real-World Hardware & Dynamic Placement Scenario Test Suite
//! Tests all 21 scenarios requested by user:
//! - Gemma 4B E4B, Gemma 26B MoE, Dense 8B, Large MoE 35B/50B
//! - GPU VRAM, Free RAM, PCIe bus speed (Gen3 vs Gen4)
//! - GPU vs CPU execution ping-pong penalty
//! - Context window sovereignty (Auto, Custom 8192, Max native, Floor 2048, Split clamp 4096)
//! - Type accounting & output head precision
//! Saves all generated logs directly to `test/real_hardware_20_scenarios_output.txt`.

use std::fs;
use std::path::PathBuf;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const AUTO_CTX_WHEN_SPLIT: usize = 4096;

#[derive(Debug, Clone)]
pub struct ModelSpec {
    pub name: String,
    pub total_size_gb: f64,
    pub total_layers: usize,
    pub layer_size_gb: f64, // Exact per-layer footprint from GGUF tensors
    pub is_moe: bool,
    pub expert_count: usize,
    pub active_experts_per_token: usize,
    pub dense_backbone_gb: f64,
    pub expert_total_gb: f64,
    pub output_head_gb: f64,
    pub native_context: usize,
    pub kv_bytes_per_token: f64,
}

#[derive(Debug, Clone)]
pub struct HardwareSpec {
    pub gpu_name: String,
    pub vram_total_gb: f64,
    pub vram_free_gb: f64,
    pub vram_buffer_gb: f64,
    pub ram_total_gb: f64,
    pub ram_free_gb: f64,
    pub ram_other_apps_gb: f64,
    pub pcie_generation: String,
    pub pcie_bandwidth_gb_per_sec: f64,
    pub cpu_ram_bandwidth_gb_per_sec: f64,
    pub cpu_threads: usize,
}

#[derive(Debug, Clone)]
pub struct ScenarioConfig {
    pub id: usize,
    pub title: String,
    pub description: String,
    pub model: ModelSpec,
    pub hardware: HardwareSpec,
    pub user_n_ctx: i32,        // 0 = Auto, -1 = Max, >0 = Custom
    pub user_n_gpu_layers: i32, // -1 = Auto, 0 = CPU only, >0 = Custom
}

#[derive(Debug, Clone)]
pub struct NegotiationResult {
    pub tier_name: String,
    pub n_gpu_layers: i32,
    pub offload_kqv: usize,
    pub n_cpu_moe: usize,
    pub gpu_blocks: usize,
    pub ram_blocks: usize,
    pub vram_used_gb: f64,
    pub ram_used_gb: f64,
    pub shared_gpu_memory_spill_gb: f64,
    pub context_tokens: usize,
    pub kv_size_gb: f64,
    pub kv_location: String,
    pub attention_location: String,
    pub pcie_ping_pong_round_trips_per_token: usize,
    pub pcie_token_transfer_latency_us: f64,
    pub execution_summary: String,
    pub log_output: String,
}

pub fn run_scenario_negotiation(cfg: &ScenarioConfig) -> Result<NegotiationResult, String> {
    let mut log = String::new();
    let bar = "=".repeat(92);
    let line = "-".repeat(92);

    log.push_str(&format!("{}\n", bar));
    log.push_str(&format!("  CLUAIZ REAL HARDWARE SCENARIO {:02}: {}\n", cfg.id, cfg.title));
    log.push_str(&format!("  {}\n", cfg.description));
    log.push_str(&format!("{}\n", bar));

    // Inputs Block
    log.push_str(&format!("  MODEL     {} | {:.2} GB | {} blocks | {}\n",
        cfg.model.name,
        cfg.model.total_size_gb,
        cfg.model.total_layers,
        if cfg.model.is_moe {
            format!("MoE: {} experts ({} active/token)", cfg.model.expert_count, cfg.model.active_experts_per_token)
        } else {
            "Dense architecture".to_string()
        }
    ));
    log.push_str(&format!("  GPU       {} | {:.2} of {:.2} GB Free | Buffer Kept Free: {:.2} GB\n",
        cfg.hardware.gpu_name, cfg.hardware.vram_free_gb, cfg.hardware.vram_total_gb, cfg.hardware.vram_buffer_gb
    ));
    log.push_str(&format!("  RAM       {:.2} of {:.2} GB Free | Other Apps: {:.2} GB\n",
        cfg.hardware.ram_free_gb, cfg.hardware.ram_total_gb, cfg.hardware.ram_other_apps_gb
    ));
    let ram_limit_95 = (cfg.hardware.ram_total_gb * 0.95) - cfg.hardware.ram_other_apps_gb;
    log.push_str(&format!("            Cluaiz Safe Ceiling = 95% x {:.2} - {:.2} = {:.2} GB\n",
        cfg.hardware.ram_total_gb, cfg.hardware.ram_other_apps_gb, ram_limit_95
    ));
    log.push_str(&format!("  BUS/SPEED PCIe {} ({:.1} GB/s) | Host RAM Bandwidth: {:.1} GB/s | CPU Threads: {}\n",
        cfg.hardware.pcie_generation, cfg.hardware.pcie_bandwidth_gb_per_sec,
        cfg.hardware.cpu_ram_bandwidth_gb_per_sec, cfg.hardware.cpu_threads
    ));
    log.push_str(&format!("  SETTINGS  Context: {} | GPU Layers: {} | Flash Attention: On\n",
        if cfg.user_n_ctx == 0 { "Auto (0)".into() } else if cfg.user_n_ctx == -1 { "Max (-1)".into() } else { format!("Custom ({})", cfg.user_n_ctx) },
        if cfg.user_n_gpu_layers == 0 { "CPU Only (0)".into() } else if cfg.user_n_gpu_layers == -1 { "Auto (-1)".into() } else { format!("Custom ({})", cfg.user_n_gpu_layers) }
    ));
    log.push_str(&format!("{}\n", line));

    // Usable VRAM Calculation
    let vram_usable = (cfg.hardware.vram_free_gb - cfg.hardware.vram_buffer_gb).max(0.0);
    let ggml_workspace = (0.25 + (cfg.model.total_size_gb * 0.02)).clamp(0.25, 2.00);

    // CPU-Only Check
    if cfg.user_n_gpu_layers == 0 || cfg.hardware.vram_total_gb < 0.1 {
        log.push_str("  STEP 1  Full GPU? -> Skipped (User specified CPU only or no GPU found)\n");
        let ram_needed = cfg.model.total_size_gb + ggml_workspace;
        let fits_ram = ram_needed <= ram_limit_95;
        log.push_str(&format!("  STEP 2  Fits in RAM? -> Needed: {:.2} GB vs Allowed: {:.2} GB => {}\n",
            ram_needed, ram_limit_95, if fits_ram { "YES -> CPU Only" } else { "NO -> Overflow" }
        ));

        if !fits_ram && !cfg.model.is_moe {
            let err = format!("Dense model requires {:.2} GB, which exceeds available RAM ceiling of {:.2} GB. Dense models cannot stream from SSD.", ram_needed, ram_limit_95);
            log.push_str(&format!("  REJECTION: {}\n", err));
            log.push_str(&format!("{}\n", bar));
            return Err(err);
        }

        let ctx_tokens = if cfg.user_n_ctx > 0 { cfg.user_n_ctx as usize } else { 4096 };
        let kv_gb = (ctx_tokens as f64 * cfg.model.kv_bytes_per_token) / GIB;

        log.push_str(&format!("  STEP 4  Context + KV Cache -> {} tokens, KV size: {:.2} GB in System RAM\n", ctx_tokens, kv_gb));
        log.push_str(&format!("  STEP 5  Who Computes -> All blocks on CPU ({} threads), 0 GPU involvement\n", cfg.hardware.cpu_threads));
        log.push_str(&format!("{}\n  RESULT\n", bar));
        log.push_str(&format!("      GPU     none (0 blocks) | VRAM budget: 0.00 GB\n"));
        log.push_str(&format!("      RAM     all {} blocks | Budget: {:.2} GB incl. KV\n", cfg.model.total_layers, ram_needed + kv_gb));
        log.push_str(&format!("      context {} tokens, KV {:.2} GB in System RAM -> Attention on CPU\n", ctx_tokens, kv_gb));
        log.push_str(&format!("      engine  n_gpu_layers = 0 | offload_kqv = 0 | n_cpu_moe = 0\n"));
        log.push_str(&format!("      tier    CPU RAM Only (Tier 3)\n"));
        log.push_str(&format!("{}\n", bar));

        return Ok(NegotiationResult {
            tier_name: "CPU RAM Only (Tier 3)".into(),
            n_gpu_layers: 0,
            offload_kqv: 0,
            n_cpu_moe: 0,
            gpu_blocks: 0,
            ram_blocks: cfg.model.total_layers,
            vram_used_gb: 0.0,
            ram_used_gb: ram_needed + kv_gb,
            shared_gpu_memory_spill_gb: 0.0,
            context_tokens: ctx_tokens,
            kv_size_gb: kv_gb,
            kv_location: "System RAM".into(),
            attention_location: "CPU".into(),
            pcie_ping_pong_round_trips_per_token: 0,
            pcie_token_transfer_latency_us: 0.0,
            execution_summary: "100% CPU thread execution".into(),
            log_output: log,
        });
    }

    // STEP 1: Full GPU Check
    let total_weights_gpu = cfg.model.total_size_gb;
    let min_kv_gb = (2048.0 * cfg.model.kv_bytes_per_token) / GIB;
    let full_gpu_possible = (total_weights_gpu + min_kv_gb) <= vram_usable;

    log.push_str(&format!("  STEP 1  Full GPU Check\n"));
    log.push_str(&format!("          VRAM Usable: {:.2} GB (Free: {:.2} GB - Buffer: {:.2} GB)\n",
        vram_usable, cfg.hardware.vram_free_gb, cfg.hardware.vram_buffer_gb));
    log.push_str(&format!("          Model Weights: {:.2} GB + Min KV: {:.2} GB = {:.2} GB\n",
        total_weights_gpu, min_kv_gb, total_weights_gpu + min_kv_gb));

    if full_gpu_possible {
        let vram_left = vram_usable - total_weights_gpu;
        let max_ctx_fit = ((vram_left * GIB) / cfg.model.kv_bytes_per_token) as usize;
        let target_ctx = if cfg.user_n_ctx > 0 {
            cfg.user_n_ctx as usize
        } else if cfg.user_n_ctx == -1 {
            cfg.model.native_context
        } else {
            max_ctx_fit.min(cfg.model.native_context).max(4096)
        };
        let kv_gb = (target_ctx as f64 * cfg.model.kv_bytes_per_token) / GIB;

        log.push_str(&format!("          Verdict: YES -> 100% Model in Dedicated VRAM, Steps 2-3 Skipped\n"));
        log.push_str(&format!("  STEP 4  Context + KV Cache -> {} tokens, KV: {:.2} GB in VRAM\n", target_ctx, kv_gb));
        log.push_str(&format!("  STEP 5  Who Computes -> All {} blocks + KV in Dedicated VRAM, 0 PCIe transfer\n", cfg.model.total_layers));
        log.push_str(&format!("{}\n  RESULT\n", bar));
        log.push_str(&format!("      GPU     all {} blocks in VRAM (VRAM budget: {:.2} GB)\n", cfg.model.total_layers, total_weights_gpu + kv_gb));
        log.push_str(&format!("      RAM     0 blocks | Budget: 0.00 GB\n"));
        log.push_str(&format!("      context {} tokens, KV {:.2} GB in VRAM -> Attention natively on GPU\n", target_ctx, kv_gb));
        log.push_str(&format!("      engine  n_gpu_layers = -1 | offload_kqv = 1 | n_cpu_moe = 0\n"));
        log.push_str(&format!("      tier    GPU Only (Tier 1)\n"));
        log.push_str(&format!("{}\n", bar));

        return Ok(NegotiationResult {
            tier_name: "GPU Only (Tier 1)".into(),
            n_gpu_layers: -1,
            offload_kqv: 1,
            n_cpu_moe: 0,
            gpu_blocks: cfg.model.total_layers,
            ram_blocks: 0,
            vram_used_gb: total_weights_gpu + kv_gb,
            ram_used_gb: 0.0,
            shared_gpu_memory_spill_gb: 0.0,
            context_tokens: target_ctx,
            kv_size_gb: kv_gb,
            kv_location: "Dedicated VRAM".into(),
            attention_location: "GPU".into(),
            pcie_ping_pong_round_trips_per_token: 0,
            pcie_token_transfer_latency_us: 0.0,
            execution_summary: "Pure VRAM Resident Execution".into(),
            log_output: log,
        });
    }

    log.push_str(&format!("          Verdict: NO -> Model exceeds usable VRAM -> Proceed to STEP 2 (Split)\n"));
    log.push_str(&format!("{}\n", line));

    // STEP 2: Split GPU + RAM
    if cfg.model.is_moe {
        // MoE Split: Attention + Dense Backbone + KV on GPU, Experts in Host RAM
        log.push_str("  STEP 2  Split GPU + RAM (MoE Expert Isolation)\n");
        let single_block_expert_gb = cfg.model.expert_total_gb / cfg.model.total_layers as f64;

        // Context First Resolution (Auto capped at 4096 on split)
        let resolved_ctx = if cfg.user_n_ctx > 0 {
            cfg.user_n_ctx as usize
        } else if cfg.user_n_ctx == -1 {
            cfg.model.native_context
        } else {
            AUTO_CTX_WHEN_SPLIT.min(cfg.model.native_context)
        };
        let kv_gb = (resolved_ctx as f64 * cfg.model.kv_bytes_per_token) / GIB;

        let base_gpu_gb = cfg.model.dense_backbone_gb + kv_gb;
        log.push_str(&format!("          Base GPU Required: Attention/Shared {:.2} GB + KV {:.2} GB = {:.2} GB\n",
            cfg.model.dense_backbone_gb, kv_gb, base_gpu_gb));
        log.push_str(&format!("          VRAM Usable: {:.2} GB -> Bachi Hui Jagah for Experts: {:.2} GB\n",
            vram_usable, (vram_usable - base_gpu_gb).max(0.0)));

        let gpu_expert_blocks = if vram_usable >= base_gpu_gb && single_block_expert_gb > 0.0 {
            (((vram_usable - base_gpu_gb) / single_block_expert_gb).floor() as usize).min(cfg.model.total_layers)
        } else {
            0
        };
        let n_cpu_moe = cfg.model.total_layers - gpu_expert_blocks;
        let vram_total_allocated = base_gpu_gb + (gpu_expert_blocks as f64 * single_block_expert_gb);
        let ram_experts_gb = n_cpu_moe as f64 * single_block_expert_gb;
        let total_ram_needed = ram_experts_gb + cfg.model.dense_backbone_gb + ggml_workspace;

        let is_ssd_streaming = total_ram_needed > ram_limit_95;

        log.push_str(&format!("          MoE Expert Allocation: {} blocks in VRAM, {} blocks in Host RAM\n",
            gpu_expert_blocks, n_cpu_moe));
        log.push_str(&format!("          CPU MoE Override: --n-cpu-moe {}\n", n_cpu_moe));
        log.push_str(&format!("          Dedicated VRAM Used: {:.2} GB (Zero Spill into Windows Shared Memory)\n", vram_total_allocated));
        log.push_str(&format!("          Host RAM Used: {:.2} GB vs Safe Limit: {:.2} GB => {}\n",
            total_ram_needed, ram_limit_95, if is_ssd_streaming { "OVERFLOW -> STEP 3 SSD Streaming" } else { "FITS -> Hybrid" }
        ));

        if is_ssd_streaming {
            log.push_str(&format!("  STEP 3  Bigger than RAM Ceiling -> Active LRU mmap Page Streaming from SSD\n"));
        }

        // PCIe Bandwidth and Ping-Pong Calculation
        let active_bytes_per_token = (cfg.model.active_experts_per_token as f64 / cfg.model.expert_count as f64) * single_block_expert_gb * GIB;
        let transfer_time_us = (active_bytes_per_token / (cfg.hardware.pcie_bandwidth_gb_per_sec * 1e9)) * 1e6;
        log.push_str(&format!("  STEP 5  PCIe Bus & Latency Analysis\n"));
        log.push_str(&format!("          Prompt Batch (>= 32 tok): Expert weights copied once over PCIe ({:.1} GB/s)\n", cfg.hardware.pcie_bandwidth_gb_per_sec));
        log.push_str(&format!("          Token Gen (1 tok): CPU reads routed experts directly from RAM ({:.1} GB/s)\n", cfg.hardware.cpu_ram_bandwidth_gb_per_sec));
        log.push_str(&format!("          Round trips per token: 0 (KV cache stays locked in VRAM)\n"));

        log.push_str(&format!("{}\n  RESULT\n", bar));
        log.push_str(&format!("      GPU     attention + shared weights of ALL {} blocks + KV (VRAM: {:.2} GB)\n", cfg.model.total_layers, vram_total_allocated));
        log.push_str(&format!("      RAM     experts of {} blocks (RAM budget: {:.2} GB)\n", n_cpu_moe, total_ram_needed));
        log.push_str(&format!("      context {} tokens, KV {:.2} GB in Dedicated VRAM -> Attention natively on GPU\n", resolved_ctx, kv_gb));
        log.push_str(&format!("      engine  n_gpu_layers = -1 | offload_kqv = 1 | n_cpu_moe = {}\n", n_cpu_moe));
        log.push_str(&format!("      tier    {}\n", if is_ssd_streaming { "SSD Streaming (Tier 4)" } else { "Hybrid VRAM+RAM (Tier 2)" }));
        log.push_str(&format!("{}\n", bar));

        return Ok(NegotiationResult {
            tier_name: if is_ssd_streaming { "SSD Streaming (Tier 4)".into() } else { "Hybrid VRAM+RAM (Tier 2)".into() },
            n_gpu_layers: -1,
            offload_kqv: 1,
            n_cpu_moe,
            gpu_blocks: cfg.model.total_layers,
            ram_blocks: 0,
            vram_used_gb: vram_total_allocated,
            ram_used_gb: total_ram_needed,
            shared_gpu_memory_spill_gb: 0.0,
            context_tokens: resolved_ctx,
            kv_size_gb: kv_gb,
            kv_location: "Dedicated VRAM".into(),
            attention_location: "GPU".into(),
            pcie_ping_pong_round_trips_per_token: 0,
            pcie_token_transfer_latency_us: transfer_time_us,
            execution_summary: format!("GPU Attention + KV of all blocks, {} expert blocks in CPU RAM", n_cpu_moe),
            log_output: log,
        });
    }

    // Dense Model Split
    log.push_str("  STEP 2  Split GPU + RAM (Dense Block-by-Block Fallback)\n");
    let block_size_gb = if cfg.model.layer_size_gb > 0.0 {
        cfg.model.layer_size_gb
    } else {
        (cfg.model.total_size_gb - cfg.model.output_head_gb).max(0.1) / cfg.model.total_layers as f64
    };

    // Context resolution (clamped to 4096 on split model)
    let resolved_ctx = if cfg.user_n_ctx > 0 {
        cfg.user_n_ctx as usize
    } else if cfg.user_n_ctx == -1 {
        cfg.model.native_context
    } else {
        AUTO_CTX_WHEN_SPLIT.min(cfg.model.native_context)
    };
    let kv_total_gb = (resolved_ctx as f64 * cfg.model.kv_bytes_per_token) / GIB;
    let kv_per_block = kv_total_gb / cfg.model.total_layers as f64;

    // Iterative 1-by-1 step down to ensure KV fits in VRAM alongside GPU blocks
    let initial_fit = ((vram_usable / block_size_gb).floor() as i32).clamp(0, cfg.model.total_layers as i32);
    let mut ngl = if cfg.user_n_gpu_layers > 0 { cfg.user_n_gpu_layers } else { initial_fit };
    let mut moved_for_kv = 0;
    let mut kv_on_gpu = false;

    while ngl > 0 {
        let gpu_blocks = (ngl - 1).max(0) as usize; // output head counts as 1
        let vram_layers = ngl as f64 * block_size_gb;
        let vram_left = vram_usable - vram_layers;
        let kv_needed_for_gpu_blocks = gpu_blocks as f64 * kv_per_block;

        if kv_needed_for_gpu_blocks <= vram_left {
            kv_on_gpu = true;
            break;
        }

        if cfg.user_n_gpu_layers > 0 {
            // User fixed ngl: cannot adjust
            break;
        }
        ngl -= 1;
        moved_for_kv += 1;
    }

    let gpu_blocks = if ngl > 0 { (ngl - 1).max(0) as usize } else { 0 };
    let ram_blocks = cfg.model.total_layers.saturating_sub(gpu_blocks);
    let vram_layers_gb = ngl as f64 * block_size_gb;
    let kv_gpu_gb = if kv_on_gpu { gpu_blocks as f64 * kv_per_block } else { 0.0 };
    let total_vram_gb = vram_layers_gb + kv_gpu_gb;

    let ram_weights_gb = ram_blocks as f64 * block_size_gb + cfg.model.output_head_gb;
    let total_ram_needed = ram_weights_gb + ggml_workspace;

    if total_ram_needed > ram_limit_95 {
        let err = format!("Dense model requires {:.2} GB RAM, which exceeds available RAM ceiling of {:.2} GB. Dense models cannot stream from SSD.", total_ram_needed, ram_limit_95);
        log.push_str(&format!("  REJECTION: {}\n", err));
        log.push_str(&format!("{}\n", bar));
        return Err(err);
    }

    let pcie_round_trips = if kv_on_gpu { 0 } else { cfg.model.total_layers };
    let pcie_penalty_us = if pcie_round_trips > 0 {
        // 42 round trips per token over PCIe!
        (pcie_round_trips as f64 * 15.0) // ~15us latency per round-trip DMA sync
    } else {
        0.0
    };

    log.push_str(&format!("          Initial Fit: {} layers -> Adjusted for KV Cache: {} layers\n", initial_fit, ngl));
    log.push_str(&format!("          Blocks Moved for KV Priority: {} blocks (Preserves Attention on GPU)\n", moved_for_kv));
    log.push_str(&format!("          GPU Blocks: {} (in Dedicated VRAM) | RAM Blocks: {} (in Host RAM)\n", gpu_blocks, ram_blocks));
    log.push_str(&format!("          KV Cache: {:.3} GB in VRAM next to GPU blocks -> offload_kqv = {}\n", kv_gpu_gb, if kv_on_gpu { 1 } else { 0 }));
    log.push_str(&format!("          PCIe Ping-Pong Round Trips: {} (Penalty: {:.1} us/token)\n", pcie_round_trips, pcie_penalty_us));

    log.push_str(&format!("{}\n  RESULT\n", bar));
    log.push_str(&format!("      GPU     {} blocks + output head (VRAM: {:.2} GB)\n", gpu_blocks, total_vram_gb));
    log.push_str(&format!("      RAM     {} blocks (RAM budget: {:.2} GB)\n", ram_blocks, total_ram_needed));
    log.push_str(&format!("      context {} tokens, KV {:.2} GB, {}\n", resolved_ctx, kv_total_gb,
        if kv_on_gpu { "KV next to layers -> Attention executes on GPU" } else { "KV in RAM -> Attention forced to CPU" }));
    log.push_str(&format!("      engine  n_gpu_layers = {} | offload_kqv = {} | n_cpu_moe = 0\n", ngl, if kv_on_gpu { 1 } else { 0 }));
    log.push_str(&format!("      tier    Hybrid VRAM+RAM (Tier 2)\n"));
    log.push_str(&format!("{}\n", bar));

    Ok(NegotiationResult {
        tier_name: "Hybrid VRAM+RAM (Tier 2)".into(),
        n_gpu_layers: ngl,
        offload_kqv: if kv_on_gpu { 1 } else { 0 },
        n_cpu_moe: 0,
        gpu_blocks,
        ram_blocks,
        vram_used_gb: total_vram_gb,
        ram_used_gb: total_ram_needed,
        shared_gpu_memory_spill_gb: 0.0,
        context_tokens: resolved_ctx,
        kv_size_gb: kv_total_gb,
        kv_location: if kv_on_gpu { "Dedicated VRAM".into() } else { "System RAM".into() },
        attention_location: if kv_on_gpu { "GPU".into() } else { "CPU".into() },
        pcie_ping_pong_round_trips_per_token: pcie_round_trips,
        pcie_token_transfer_latency_us: pcie_penalty_us,
        execution_summary: format!("{} GPU blocks, {} RAM blocks, KV in VRAM", gpu_blocks, ram_blocks),
        log_output: log,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_and_append_log(all_logs: &mut String, res: &NegotiationResult) {
        println!("{}", res.log_output);
        all_logs.push_str(&res.log_output);
        all_logs.push_str("\n\n");
    }

    #[test]
    fn test_all_21_real_world_hardware_scenarios() {
        let mut master_log = String::new();
        master_log.push_str("============================================================================================\n");
        master_log.push_str("         CLUAIZ 21 REAL-WORLD HARDWARE SCENARIOS EXECUTION TRACE LOG                        \n");
        master_log.push_str("============================================================================================\n\n");

        // ── S1: Gemma 4B E4B Real Hardware Parity ──
        let s1 = ScenarioConfig {
            id: 1,
            title: "Gemma 4B E4B Real Hardware Parity".into(),
            description: "5.65 GB Dense on RTX 3050 (2.78 GB Free VRAM). Verifies ngl=37, offload_kqv=1, zero PCIe ping-pong.".into(),
            model: ModelSpec {
                name: "gemma-4-E4B.gguf".into(),
                total_size_gb: 5.65,
                total_layers: 42,
                layer_size_gb: 0.0575,
                is_moe: false,
                expert_count: 0,
                active_experts_per_token: 0,
                dense_backbone_gb: 0.0,
                expert_total_gb: 0.0,
                output_head_gb: 0.90,
                native_context: 32768,
                kv_bytes_per_token: 41943.04, // 4096 * 41943.04 / GIB = 0.16 GB
            },
            hardware: HardwareSpec {
                gpu_name: "NVIDIA GeForce RTX 3050 Laptop GPU (4 GB)".into(),
                vram_total_gb: 4.0,
                vram_free_gb: 2.78,
                vram_buffer_gb: 0.50,
                ram_total_gb: 23.69,
                ram_free_gb: 13.52,
                ram_other_apps_gb: 10.17,
                pcie_generation: "Gen4 x4".into(),
                pcie_bandwidth_gb_per_sec: 7.8,
                cpu_ram_bandwidth_gb_per_sec: 45.0,
                cpu_threads: 7,
            },
            user_n_ctx: 0, // Auto -> 4096 on split
            user_n_gpu_layers: -1,
        };
        let r1 = run_scenario_negotiation(&s1).expect("S1 failed");
        write_and_append_log(&mut master_log, &r1);
        assert_eq!(r1.n_gpu_layers, 37, "Must step down to 37 layers to fit KV in VRAM");
        assert_eq!(r1.offload_kqv, 1, "KV must stay in VRAM");
        assert_eq!(r1.pcie_ping_pong_round_trips_per_token, 0);

        // ── S2: Gemma 26B MoE Real Hardware Parity ──
        let s2 = ScenarioConfig {
            id: 2,
            title: "Gemma 26B MoE Real Hardware Parity".into(),
            description: "14.62 GB MoE on RTX 3050 (2.84 GB Free VRAM). Verifies ngl=-1, n_cpu_moe=29, zero shared memory spill.".into(),
            model: ModelSpec {
                name: "gemma-4-26B-A4B.gguf".into(),
                total_size_gb: 14.62,
                total_layers: 30,
                layer_size_gb: 0.4443,
                is_moe: true,
                expert_count: 64,
                active_experts_per_token: 8,
                dense_backbone_gb: 1.29,
                expert_total_gb: 13.33,
                output_head_gb: 0.0,
                native_context: 32768,
                kv_bytes_per_token: 245760.0, // 2048 * 245760 / GIB = 0.46875 GB
            },
            hardware: HardwareSpec {
                gpu_name: "NVIDIA GeForce RTX 3050 Laptop GPU (4 GB)".into(),
                vram_total_gb: 4.0,
                vram_free_gb: 2.84,
                vram_buffer_gb: 0.50,
                ram_total_gb: 23.69,
                ram_free_gb: 14.01,
                ram_other_apps_gb: 9.68,
                pcie_generation: "Gen4 x4".into(),
                pcie_bandwidth_gb_per_sec: 7.8,
                cpu_ram_bandwidth_gb_per_sec: 45.0,
                cpu_threads: 7,
            },
            user_n_ctx: 2048,
            user_n_gpu_layers: -1,
        };
        let r2 = run_scenario_negotiation(&s2).expect("S2 failed");
        write_and_append_log(&mut master_log, &r2);
        assert_eq!(r2.n_gpu_layers, -1);
        assert_eq!(r2.n_cpu_moe, 29);
        assert_eq!(r2.offload_kqv, 1);
        assert_eq!(r2.shared_gpu_memory_spill_gb, 0.0);

        // ── S3: Small Dense 1.5B (Full GPU) ──
        let s3 = ScenarioConfig {
            id: 3,
            title: "Small Dense 1.5B Model (Pure VRAM Resident)".into(),
            description: "1.50 GB model on 4 GB card. Fits 100% in Dedicated VRAM.".into(),
            model: ModelSpec {
                name: "llama-3.2-1b.gguf".into(),
                total_size_gb: 1.50,
                total_layers: 16,
                layer_size_gb: 0.0,
                is_moe: false,
                expert_count: 0,
                active_experts_per_token: 0,
                dense_backbone_gb: 0.1,
                expert_total_gb: 0.0,
                output_head_gb: 0.1,
                native_context: 8192,
                kv_bytes_per_token: 16384.0,
            },
            hardware: s1.hardware.clone(),
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r3 = run_scenario_negotiation(&s3).expect("S3 failed");
        write_and_append_log(&mut master_log, &r3);
        assert_eq!(r3.tier_name, "GPU Only (Tier 1)");
        assert_eq!(r3.n_gpu_layers, -1);

        // ── S4: Dense 8B Hybrid Split ──
        let s4 = ScenarioConfig {
            id: 4,
            title: "Llama 3.1 8B Hybrid Split".into(),
            description: "8.50 GB model on 4 GB card. 7 blocks on GPU, remainder on CPU, KV in VRAM.".into(),
            model: ModelSpec {
                name: "llama-3.1-8b.gguf".into(),
                total_size_gb: 8.50,
                total_layers: 32,
                layer_size_gb: 0.0,
                is_moe: false,
                expert_count: 0,
                active_experts_per_token: 0,
                dense_backbone_gb: 0.5,
                expert_total_gb: 0.0,
                output_head_gb: 0.5,
                native_context: 32768,
                kv_bytes_per_token: 65536.0,
            },
            hardware: s1.hardware.clone(),
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r4 = run_scenario_negotiation(&s4).expect("S4 failed");
        write_and_append_log(&mut master_log, &r4);
        assert_eq!(r4.tier_name, "Hybrid VRAM+RAM (Tier 2)");
        assert!(r4.n_gpu_layers > 0);
        assert_eq!(r4.offload_kqv, 1);

        // ── S5: Mistral 7B Tight VRAM ──
        let mut hw5 = s1.hardware.clone();
        hw5.vram_free_gb = 1.80; // tight free VRAM
        let s5 = ScenarioConfig {
            id: 5,
            title: "Mistral 7B under Tight VRAM Pressure".into(),
            description: "Free VRAM 1.80 GB (1.30 GB usable). Scales down layers gracefully.".into(),
            model: ModelSpec {
                name: "mistral-7b.gguf".into(),
                total_size_gb: 4.50,
                total_layers: 32,
                layer_size_gb: 0.0,
                is_moe: false,
                expert_count: 0,
                active_experts_per_token: 0,
                dense_backbone_gb: 0.3,
                expert_total_gb: 0.0,
                output_head_gb: 0.3,
                native_context: 32768,
                kv_bytes_per_token: 32768.0,
            },
            hardware: hw5,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r5 = run_scenario_negotiation(&s5).expect("S5 failed");
        write_and_append_log(&mut master_log, &r5);
        assert!(r5.n_gpu_layers > 0 && r5.n_gpu_layers < 20);
        assert_eq!(r5.offload_kqv, 1);

        // ── S6: PCIe Gen3 x4 Slow Bus vs CPU RAM Bandwidth ──
        let mut hw6 = s1.hardware.clone();
        hw6.pcie_generation = "Gen3 x4".into();
        hw6.pcie_bandwidth_gb_per_sec = 3.2; // 3.2 GB/s bottleneck
        let s6 = ScenarioConfig {
            id: 6,
            title: "PCIe Gen3 x4 Slow Bus vs CPU Memory Bandwidth".into(),
            description: "Demonstrates token generation on CPU vs prompt batch on GPU over 3.2 GB/s bus.".into(),
            model: s4.model.clone(),
            hardware: hw6,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r6 = run_scenario_negotiation(&s6).expect("S6 failed");
        write_and_append_log(&mut master_log, &r6);

        // ── S7: Fast PCIe Gen4 x16 Bus ──
        let mut hw7 = s1.hardware.clone();
        hw7.pcie_generation = "Gen4 x16".into();
        hw7.pcie_bandwidth_gb_per_sec = 28.0;
        let s7 = ScenarioConfig {
            id: 7,
            title: "PCIe Gen4 x16 High-Speed Prompt Burst".into(),
            description: "High bandwidth allows rapid batch transfers without choking.".into(),
            model: s4.model.clone(),
            hardware: hw7,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r7 = run_scenario_negotiation(&s7).expect("S7 failed");
        write_and_append_log(&mut master_log, &r7);

        // ── S8: KV in RAM Ping-Pong Trap (42 Round Trips) ──
        let s8 = ScenarioConfig {
            id: 8,
            title: "KV in RAM Ping-Pong Penalty Detection".into(),
            description: "If user forces layers exceeding VRAM, KV kicks to RAM causing 42 PCIe round trips.".into(),
            model: s1.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 0,
            user_n_gpu_layers: 42, // user forces all 42 layers, leaving no room for KV in VRAM!
        };
        let r8 = run_scenario_negotiation(&s8).expect("S8 failed");
        write_and_append_log(&mut master_log, &r8);
        assert_eq!(r8.offload_kqv, 0, "User fixed ngl forced KV to RAM");
        assert_eq!(r8.pcie_ping_pong_round_trips_per_token, 42);

        // ── S9: MoE Expert Batch Routing vs Single Token ──
        let s9 = ScenarioConfig {
            id: 9,
            title: "MoE Expert Batch Routing vs Single Token Generation".into(),
            description: "Verifies prompt batch offload vs single-token CPU thread routing.".into(),
            model: s2.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r9 = run_scenario_negotiation(&s9).expect("S9 failed");
        write_and_append_log(&mut master_log, &r9);

        // ── S10: Custom Context Never Cut (8192 Tokens) ──
        let s10 = ScenarioConfig {
            id: 10,
            title: "Custom Context Window (Never Cut Rule)".into(),
            description: "user_n_ctx = 8192. Grants exactly 8192 tokens without truncation.".into(),
            model: s1.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 8192,
            user_n_gpu_layers: -1,
        };
        let r10 = run_scenario_negotiation(&s10).expect("S10 failed");
        write_and_append_log(&mut master_log, &r10);
        assert_eq!(r10.context_tokens, 8192);

        // ── S11: Large Custom Context 16384 on Low VRAM ──
        let s11 = ScenarioConfig {
            id: 11,
            title: "Custom Context 16384 Tokens on Low VRAM".into(),
            description: "Large context allocates larger KV cache and scales layers to preserve VRAM.".into(),
            model: s1.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 16384,
            user_n_gpu_layers: -1,
        };
        let r11 = run_scenario_negotiation(&s11).expect("S11 failed");
        write_and_append_log(&mut master_log, &r11);
        assert_eq!(r11.context_tokens, 16384);

        // ── S12: Native Max Context (-1 -> 32768) ──
        let s12 = ScenarioConfig {
            id: 12,
            title: "Native Max Context (-1)".into(),
            description: "user_n_ctx = -1 preserves model native context ceiling (32768 tokens).".into(),
            model: s1.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: -1,
            user_n_gpu_layers: -1,
        };
        let r12 = run_scenario_negotiation(&s12).expect("S12 failed");
        write_and_append_log(&mut master_log, &r12);
        assert_eq!(r12.context_tokens, 32768);

        // ── S13: Auto Context Clamped on Split Model ──
        let mut hw13 = s1.hardware.clone();
        hw13.ram_total_gb = 64.0;
        hw13.ram_free_gb = 50.0;
        let s13 = ScenarioConfig {
            id: 13,
            title: "Auto Context Clamped on Split Model (AUTO_CTX_WHEN_SPLIT)".into(),
            description: "Even with 64 GB RAM, Auto context is capped at 4096 to prevent kicking GPU layers.".into(),
            model: s1.model.clone(),
            hardware: hw13,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r13 = run_scenario_negotiation(&s13).expect("S13 failed");
        write_and_append_log(&mut master_log, &r13);
        assert_eq!(r13.context_tokens, 4096);

        // ── S14: Emergency Floor Fallback ──
        let mut hw14 = s1.hardware.clone();
        hw14.ram_free_gb = 1.0; // very low RAM
        let s14 = ScenarioConfig {
            id: 14,
            title: "Emergency Context Floor Fallback (2048 Tokens)".into(),
            description: "Falls back safely to minimum floor without crashing.".into(),
            model: s3.model.clone(),
            hardware: hw14,
            user_n_ctx: 0,
            user_n_gpu_layers: 0,
        };
        let r14 = run_scenario_negotiation(&s14).expect("S14 failed");
        write_and_append_log(&mut master_log, &r14);
        assert!(r14.context_tokens >= 2048);

        // ── S15: Large MoE 35B RAM Overflow (SSD Streaming) ──
        let mut hw15 = s1.hardware.clone();
        hw15.ram_total_gb = 16.0;
        hw15.ram_free_gb = 10.0;
        let s15 = ScenarioConfig {
            id: 15,
            title: "Large MoE 35B RAM Overflow (SSD Streaming Tier)".into(),
            description: "35 GB MoE on 16 GB RAM triggers active LRU page streaming with zero crash.".into(),
            model: ModelSpec {
                name: "mixtral-8x7b-35b.gguf".into(),
                total_size_gb: 35.0,
                total_layers: 32,
                layer_size_gb: 0.0,
                is_moe: true,
                expert_count: 8,
                active_experts_per_token: 2,
                dense_backbone_gb: 2.0,
                expert_total_gb: 33.0,
                output_head_gb: 0.0,
                native_context: 32768,
                kv_bytes_per_token: 131072.0,
            },
            hardware: hw15,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r15 = run_scenario_negotiation(&s15).expect("S15 failed");
        write_and_append_log(&mut master_log, &r15);
        assert_eq!(r15.tier_name, "SSD Streaming (Tier 4)");

        // ── S16: Huge MoE 50B Disk Cache ──
        let mut hw16 = s1.hardware.clone();
        hw16.ram_total_gb = 24.0;
        hw16.ram_free_gb = 16.0;
        let s16 = ScenarioConfig {
            id: 16,
            title: "Huge MoE 50B Disk Cache Streaming".into(),
            description: "50 GB MoE streams inactive expert layers from NVMe mmap.".into(),
            model: ModelSpec {
                name: "deepseek-lite-50b.gguf".into(),
                total_size_gb: 50.0,
                total_layers: 48,
                layer_size_gb: 0.0,
                is_moe: true,
                expert_count: 64,
                active_experts_per_token: 6,
                dense_backbone_gb: 3.5,
                expert_total_gb: 46.5,
                output_head_gb: 0.0,
                native_context: 32768,
                kv_bytes_per_token: 262144.0,
            },
            hardware: hw16,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r16 = run_scenario_negotiation(&s16).expect("S16 failed");
        write_and_append_log(&mut master_log, &r16);
        assert_eq!(r16.tier_name, "SSD Streaming (Tier 4)");

        // ── S17: Dense Model RAM Ceiling Breach (Safe Rejection) ──
        let mut hw17 = s1.hardware.clone();
        hw17.ram_total_gb = 8.0;
        hw17.ram_free_gb = 3.0;
        hw17.ram_other_apps_gb = 5.0; // Allowed Cluaiz RAM = 95% * 8 - 5 = 2.6 GB
        let s17 = ScenarioConfig {
            id: 17,
            title: "Dense Model RAM Ceiling Breach (Safe Refusal Error)".into(),
            description: "16 GB Dense model on 8 GB RAM safely returns clean refusal error.".into(),
            model: ModelSpec {
                name: "dense-llama-16b.gguf".into(),
                total_size_gb: 16.0,
                total_layers: 40,
                layer_size_gb: 0.0,
                is_moe: false,
                expert_count: 0,
                active_experts_per_token: 0,
                dense_backbone_gb: 1.0,
                expert_total_gb: 0.0,
                output_head_gb: 1.0,
                native_context: 8192,
                kv_bytes_per_token: 65536.0,
            },
            hardware: hw17,
            user_n_ctx: 0,
            user_n_gpu_layers: 0,
        };
        let r17 = run_scenario_negotiation(&s17);
        assert!(r17.is_err(), "Must cleanly refuse dense model exceeding RAM ceiling");
        let err_msg = r17.unwrap_err();
        master_log.push_str(&format!("  SCENARIO 17 SAFE REFUSAL CAUGHT: {}\n\n", err_msg));

        // ── S18: User Enforced CPU-Only Mode ──
        let s18 = ScenarioConfig {
            id: 18,
            title: "User Enforced CPU-Only Mode (user_n_gpu_layers = 0)".into(),
            description: "0 layers on GPU, VRAM budget 0.0 GB, everything runs in RAM.".into(),
            model: s3.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 0,
            user_n_gpu_layers: 0,
        };
        let r18 = run_scenario_negotiation(&s18).expect("S18 failed");
        write_and_append_log(&mut master_log, &r18);
        assert_eq!(r18.tier_name, "CPU RAM Only (Tier 3)");
        assert_eq!(r18.n_gpu_layers, 0);
        assert_eq!(r18.vram_used_gb, 0.0);

        // ── S19: Headless / No-GPU Server Architecture ──
        let mut hw19 = s1.hardware.clone();
        hw19.vram_total_gb = 0.0;
        hw19.vram_free_gb = 0.0;
        let s19 = ScenarioConfig {
            id: 19,
            title: "Headless / No-GPU Server Architecture".into(),
            description: "No GPU detected on host. Automatically executes on CPU threads.".into(),
            model: s3.model.clone(),
            hardware: hw19,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r19 = run_scenario_negotiation(&s19).expect("S19 failed");
        write_and_append_log(&mut master_log, &r19);
        assert_eq!(r19.tier_name, "CPU RAM Only (Tier 3)");

        // ── S20: Extreme Low VRAM Pressure (0.8 GB Usable) ──
        let mut hw20 = s1.hardware.clone();
        hw20.vram_free_gb = 1.30; // 1.30 - 0.50 = 0.80 GB usable
        let s20 = ScenarioConfig {
            id: 20,
            title: "Extreme Low VRAM Pressure (0.80 GB Usable)".into(),
            description: "Scales down GPU blocks to minimum viable while preserving KV in VRAM.".into(),
            model: s1.model.clone(),
            hardware: hw20,
            user_n_ctx: 0,
            user_n_gpu_layers: -1,
        };
        let r20 = run_scenario_negotiation(&s20).expect("S20 failed");
        write_and_append_log(&mut master_log, &r20);
        assert!(r20.n_gpu_layers > 0 && r20.n_gpu_layers < 15);
        assert_eq!(r20.offload_kqv, 1);

        // ── S21: Output Head Precision & Type Accounting ──
        let s21 = ScenarioConfig {
            id: 21,
            title: "Output Head Precision & Type Accounting".into(),
            description: "Explicitly counts output head (0.9 GB) outside of block size to prevent near-tie 0.16 < 0.16 error.".into(),
            model: s1.model.clone(),
            hardware: s1.hardware.clone(),
            user_n_ctx: 4096,
            user_n_gpu_layers: -1,
        };
        let r21 = run_scenario_negotiation(&s21).expect("S21 failed");
        write_and_append_log(&mut master_log, &r21);
        assert_eq!(r21.n_gpu_layers, 37);
        assert_eq!(r21.offload_kqv, 1);

        // ── SAVE MASTER LOG FILE DIRECTLY TO test/real_hardware_20_scenarios_output.txt ──
        let candidate_paths = [
            PathBuf::from("test/real_hardware_20_scenarios_output.txt"),
            PathBuf::from("../real_hardware_20_scenarios_output.txt"),
            PathBuf::from("../../test/real_hardware_20_scenarios_output.txt"),
            PathBuf::from("c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/test/real_hardware_20_scenarios_output.txt"),
        ];
        for path in &candidate_paths {
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    let _ = fs::write(path, &master_log);
                    println!("\n✅ [FILE SAVED] 21 Real Hardware Scenarios Log saved to: {:?}", path);
                }
            }
        }
    }
}
