//! Placement rules checked across GPU sizes, model types, free RAM and settings. Every row of the
//! table must keep the rules below; a failure lists every row that broke a rule, not only the first.
//!
//! 1. The planned VRAM is never above the free VRAM.
//! 2. CPU only is never chosen while a GPU placement fits that reads no more from the disk.
//! 3. MoE: no expert is on the GPU while the attention of some block is in RAM.
//! 4. The KV cache is where its attention runs, unless the plan puts the whole KV in RAM.
//! 5. A placement that reads experts from the disk is never chosen over one that fits in RAM.
//! 6. More free VRAM never moves a model off the GPU, makes it stop fitting, or reads more from disk.
//!
//! The rows use the hardware the host sees: a model loaded on a GPU has llama.cpp's dry-run
//! correction (`learn_correction` runs before the plan), so most rows record one; rows without it
//! keep llama.cpp's 1024 / 512 MiB rule; a third set has a learned gap below zero (a load that
//! took less than planned).

use super::tests::{qwen3_like, settings};
use super::*;
use crate::types::PlacementTier;

const MIB: u64 = 1024 * 1024;

/// A model of the table, with a file size no other test uses (the measurements are process-wide)
fn sized(mut g: ModelGeometry, id: u64) -> ModelGeometry {
    g.file_size_bytes = 9_400_000_000 + id;
    g
}

/// MoE model: `attention_mib` (non-expert bytes) and `experts_mib` per block
fn moe(blocks: u32, attention_mib: u64, experts_mib: u64) -> ModelGeometry {
    let mut g = qwen3_like(1.0);
    let n = blocks as usize;
    g.block_count = blocks;
    g.head_count_kv_per_layer = vec![8; n];
    g.weights.layer = vec![attention_mib * MIB; n];
    g.weights.layer_experts = vec![experts_mib * MIB; n];
    g.weights.total = g.weights.input_cpu + blocks as u64 * (attention_mib + experts_mib) * MIB;
    g
}

/// Qwen3.5-like hybrid: every 4th block has attention, the others a fixed recurrent state
fn hybrid() -> ModelGeometry {
    let mut g = qwen3_like(2.59);
    g.block_count = 33;
    g.head_count_kv_per_layer = vec![4; 33];
    g.key_length = 256;
    g.value_length = 256;
    g.recurrent_layers = (0..33).map(|i| i < 32 && (i + 1) % 4 != 0).collect();
    g.recurrent_state_bytes = (3 * (4096 + 2 * 16 * 128) + 128 * 4096) * 4;
    g.weights.layer = vec![g.weights.layer[0]; 33];
    g.weights.layer_experts = vec![0; 33];
    g.weights.total = g.weights.input_cpu + 33 * g.weights.layer[0];
    g
}

/// (name, model) of every model kind in the table
fn models() -> Vec<(&'static str, ModelGeometry)> {
    vec![
        ("dense 1 GB", qwen3_like(1.08)),
        ("dense 4.5 GB", qwen3_like(4.5)),
        ("dense 18 GB", qwen3_like(18.0)),
        ("MoE 3.9 GB (OLMoE-like)", moe(16, 20, 220)),
        ("MoE 13 GB (Gemma 4 26B-like)", moe(30, 58, 393)),
        ("MoE 60 GB (gpt-oss-120b-like)", moe(36, 60, 1650)),
        ("hybrid 2.6 GB (Qwen3.5-like)", hybrid()),
    ]
}

/// GPU with `total` GB and `free` GB free (None: no GPU), and `ram_free` of 32 GB RAM free
fn machine(gpu: Option<(f64, f64)>, ram_free: f64) -> SystemInfo {
    let mut h = super::tests::hw(0.0, ram_free);
    h.memory.total_gb = 32.0;
    match gpu {
        Some((total, free)) => {
            h.gpus[0].vram_total_gb = total;
            h.gpus[0].vram_available_gb = free;
        }
        None => h.gpus.clear(),
    }
    h
}

fn is_cpu(tier: PlacementTier) -> bool {
    matches!(tier, PlacementTier::CpuOnly | PlacementTier::SsdStreaming)
}

/// Rules 1-5 for one plan; each broken rule as a line
fn broken_rules(plan: &ExecutionPlan, g: &ModelGeometry, h: &SystemInfo) -> Vec<String> {
    let mut out = Vec::new();
    if !plan.is_feasible() {
        return out;
    }
    let free_vram = h.gpus.first().map(|gpu| gpu.vram_available_gb).unwrap_or(0.0);
    let n_layers = g.block_count;
    let (gpu_blocks, _) = plan.block_split();
    // 1
    if plan.vram_budget_gb > free_vram + 1e-9 {
        out.push(format!("rule 1: VRAM {:.2} GB planned, {:.2} GB free", plan.vram_budget_gb, free_vram));
    }
    // 2
    if is_cpu(plan.selected_tier) {
        if let Some(c) = plan.candidates.iter().find(|c| c.feasible && !is_cpu(c.tier) && c.disk_stream_gb <= plan.disk_stream_gb) {
            out.push(format!("rule 2: CPU only chosen while \"{}\" fits ({})", c.name, c.reason));
        }
    }
    // 3
    let is_moe = g.weights.experts_total() > 0;
    if is_moe && gpu_blocks > 0 && gpu_blocks < n_layers && plan.n_cpu_moe < n_layers {
        out.push(format!("rule 3: experts on the GPU while {} / {} blocks' attention is in RAM (n_gpu_layers {}, n_cpu_moe {})",
            n_layers - gpu_blocks, n_layers, plan.n_gpu_layers, plan.n_cpu_moe));
    }
    if is_moe && !plan.kv_offload && plan.n_cpu_moe < n_layers && plan.n_gpu_layers != 0 {
        out.push("rule 3: experts on the GPU while the KV (attention) is in RAM".into());
    }
    // 4
    if plan.kv_offload && gpu_blocks == n_layers && plan.kv_ram_gb > 0.0 {
        out.push(format!("rule 4: every block on the GPU but {:.2} GB of KV in RAM", plan.kv_ram_gb));
    }
    if !plan.kv_offload && plan.kv_gpu_gb > 0.0 {
        out.push(format!("rule 4: KV said to be in RAM but {:.2} GB of it on the GPU", plan.kv_gpu_gb));
    }
    // 5
    if plan.disk_stream_gb > 0.0 {
        if let Some(c) = plan.candidates.iter().find(|c| c.feasible && c.disk_stream_gb == 0.0) {
            out.push(format!("rule 5: {:.2} GB read from disk while \"{}\" fits in RAM", plan.disk_stream_gb, c.name));
        }
    }
    out
}

/// One row of the rule table: its label, the model, the machine, the settings and the plan
pub(crate) struct Row {
    pub label: String,
    pub geometry: ModelGeometry,
    pub hardware: SystemInfo,
    pub settings: ActiveUserSettings,
    pub plan: ExecutionPlan,
}

/// Every plan of the rule table: models x measurement states x settings x GPU sizes x free RAM
pub(crate) fn all_plans() -> Vec<Row> {
    let cards: [Option<f64>; 7] = [None, Some(2.0), Some(4.0), Some(6.0), Some(8.0), Some(12.0), Some(24.0)];
    let rams = [("RAM plenty", 28.0), ("RAM tight", 12.0), ("RAM short", 5.0)];
    let setting_rows = [
        ("Auto", settings(ContextSetting::Auto, KvCacheQuantization::Auto)),
        ("Custom 32768", settings(ContextSetting::Custom(32768), KvCacheQuantization::Auto)),
    ];
    let mut rows = Vec::new();
    for (m, (model_name, base)) in models().into_iter().enumerate() {
        for (k, state) in ["dry run", "no dry run", "estimate measured 0.6 GB too high"].into_iter().enumerate() {
            let g = sized(base.clone(), m as u64 * 3 + k as u64);
            for (setting_name, s) in &setting_rows {
                match k {
                    0 => crate::margin::record_correction(&g, s, 0.0),
                    // A load that took less than planned: the learned gap is below zero, and must
                    // never make the planner use more VRAM than is free
                    2 => {
                        crate::margin::record_correction(&g, s, 0.0);
                        crate::margin::record(&g, s, -0.6);
                    }
                    _ => {}
                }
                for card in cards {
                    // A desktop GPU with a quarter in use by other apps
                    let gpu = card.map(|total| (total, total * 0.75));
                    for (ram_name, ram_free) in rams {
                        let h = machine(gpu, ram_free);
                        let plan = Planner::create_plan("matrix", &g, &h, s);
                        let label = format!("{model_name} | {state} | {setting_name} | GPU {} | {ram_name}",
                            card.map_or("none".to_string(), |t| format!("{t} GB")));
                        rows.push(Row { label, geometry: g.clone(), hardware: h, settings: s.clone(), plan });
                    }
                }
            }
        }
    }
    rows
}

#[test]
fn placement_rules_hold_across_gpus_models_ram_and_settings() {
    let rows = all_plans();
    let mut failures = Vec::new();
    for row in &rows {
        for rule in broken_rules(&row.plan, &row.geometry, &row.hardware) {
            failures.push(format!("{}: {rule}
    plan: {}", row.label, row.plan.explanation));
        }
    }
    assert!(rows.len() > 500, "{} rows", rows.len());
    assert!(failures.is_empty(), "{} of {} rows broke a rule:
{}", failures.len(), rows.len(), failures.join("
"));
}

#[test]
fn more_free_vram_never_gives_a_worse_placement() {
    let s = settings(ContextSetting::Auto, KvCacheQuantization::Auto);
    let mut failures = Vec::new();
    for (m, (model_name, base)) in models().into_iter().enumerate() {
        let g = sized(base, 100 + m as u64);
        crate::margin::record_correction(&g, &s, 0.0);
        for ram_free in [28.0, 12.0] {
            let mut prev: Option<ExecutionPlan> = None;
            let mut free = 0.25;
            while free <= 8.0 {
                let h = machine(Some((8.0, free)), ram_free);
                let plan = Planner::create_plan("matrix", &g, &h, &s);
                if let Some(p) = &prev {
                    let left_gpu = !is_cpu(p.selected_tier) && p.n_gpu_layers != 0 && is_cpu(plan.selected_tier);
                    let stopped_fitting = p.is_feasible() && !plan.is_feasible();
                    let more_disk = plan.disk_stream_gb > p.disk_stream_gb + 1e-9;
                    if left_gpu || stopped_fitting || more_disk {
                        failures.push(format!(
                            "{model_name} | RAM {ram_free} GB | free VRAM {:.2} -> {:.2}: {} -> {}",
                            free - 0.25, free, p.explanation, plan.explanation
                        ));
                    }
                }
                prev = Some(plan);
                free += 0.25;
            }
        }
    }
    assert!(failures.is_empty(), "rule 6 broken {} times:\n{}", failures.len(), failures.join("\n"));
}
