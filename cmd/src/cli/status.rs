//! `cluaiz status`: the hardware the planner sees (live free memory).

pub fn execute() {
    // Loading llama.cpp's backends first lets the probe list Metal and Vulkan GPUs too
    let _ = cluaiz_runtime::gguf_hardware();
    let hw = cluaiz_hw::HardwareProber::with_live_memory();
    let row = |k: &str, v: String| println!("{:<10}{}", k, v);
    row("os", format!("{} {} ({})", hw.os_name, hw.os_version, hw.architecture));
    row("cpu", format!("{} | {} cores, {} threads", hw.cpu.brand.trim(), hw.cpu.physical_cores, hw.cpu.logical_threads));
    row("ram", format!("{:.2} GB free of {:.2} GB", hw.memory.available_gb, hw.memory.total_gb));
    if hw.gpus.is_empty() {
        row("gpu", "none (CPU only)".to_string());
    }
    for gpu in &hw.gpus {
        row(
            &format!("gpu {}", gpu.index),
            format!("{} | {:.2} GB free of {:.2} GB VRAM", gpu.name, gpu.vram_available_gb, gpu.vram_total_gb),
        );
    }
}
