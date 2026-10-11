//! On-demand check of the planner against the real GGUF files installed on this machine.
//! Run: cargo test -p cluaiz-plan --test real_models -- --ignored --nocapture

use cluaiz_core::{ActiveUserSettings, ContextSetting, EnvironmentManager, KvCacheQuantization};
use cluaiz_plan::{Planner, AUTO_CONTEXT_FLOOR};

fn installed_ggufs() -> Vec<std::path::PathBuf> {
    let root = EnvironmentManager::current().models_dir().join("chat");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).into_iter().flatten().flatten() {
        for f in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let p = f.path();
            let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
            if name.ends_with(".gguf") && !name.starts_with("mmproj") && !name.starts_with("mtp-") {
                out.push(p);
            }
        }
    }
    out
}

/// Hardware as in the user's screenshots: RTX 3050 4 GB, 23.69 GB RAM.
fn hardware(vram_free: f64, ram_free: f64) -> cluaiz_hw::SystemInfo {
    let mut h = cluaiz_hw::SystemInfo::default();
    h.memory.total_gb = 23.69;
    h.memory.available_gb = ram_free;
    h.gpus.push(cluaiz_hw::GpuSubsystem { vram_total_gb: 4.0, vram_available_gb: vram_free, ..Default::default() });
    h
}

#[test]
#[ignore]
fn planner_on_installed_models() {
    let models = installed_ggufs();
    assert!(!models.is_empty(), "no installed GGUF chat models found");
    for path in &models {
        let g = cluaiz_models::GgufProber::probe_geometry(path).expect("probe");
        for (label, ctx) in [("Auto", ContextSetting::Auto), ("Max", ContextSetting::Max), ("8192", ContextSetting::Custom(8192))] {
            let mut s = ActiveUserSettings::default();
            s.optimization.set_context_setting(ctx);
            s.optimization.kv_cache_quantization = KvCacheQuantization::Auto;
            let plan = Planner::create_plan(&path.file_name().unwrap().to_string_lossy(), &g, &hardware(2.79, 5.5), &s);
            let (gpu_blocks, ram_blocks) = plan.block_split();
            println!(
                "{:<40} {:>5} | {:?} ngl {} ncpumoe {} | ctx {} KV {:?} | VRAM {:.2} (KV {:.2}) RAM {:.2} disk {:.2} | blocks {}/{} | buf {:.2}",
                plan.model_id, label, plan.selected_tier, plan.n_gpu_layers, plan.n_cpu_moe,
                plan.context_window, plan.kv_cache_type, plan.vram_budget_gb, plan.kv_gpu_gb,
                plan.ram_budget_gb, plan.disk_stream_gb, gpu_blocks, ram_blocks, plan.vram_buffer_gb
            );
            if !plan.is_feasible() {
                println!("    -> {}", plan.explanation);
            }
            if ctx == ContextSetting::Auto {
                assert!(plan.context_window >= AUTO_CONTEXT_FLOOR.min(g.context_length), "Auto below floor for {:?}", path);
                assert!(plan.vram_budget_gb + plan.vram_buffer_gb <= 2.79 + 1e-9 || plan.vram_budget_gb == 0.0,
                    "plan uses more VRAM than free for {:?}", path);
            }
        }
    }
}
