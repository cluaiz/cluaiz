//! `cluaiz status`: the hardware the planner sees (live free memory).

pub fn execute() {
    // Loading llama.cpp's backends first lets the probe list Metal and Vulkan GPUs too
    let _ = cluaiz_runtime::gguf_hardware();
    let backends = cluaiz_runtime::loaded_gpu_backends();
    let hw = cluaiz_hw::HardwareProber::with_live_memory();
    let row = |k: &str, v: String| println!("{:<10}{}", k, v);
    row("os", format!("{} {} ({})", hw.os_name, hw.os_version, hw.architecture));
    row("cpu", format!("{} | {} cores, {} threads", hw.cpu.brand.trim(), hw.cpu.physical_cores, hw.cpu.logical_threads));
    let ram_note = if hw.memory.is_unified_memory && !hw.gpus.is_empty() { " (shared with the GPU)" } else { "" };
    row("ram", format!("{:.2} GB free of {:.2} GB{}", hw.memory.available_gb, hw.memory.total_gb, ram_note));
    if hw.gpus.is_empty() {
        row("gpu", "none (CPU only)".to_string());
    }
    for gpu in &hw.gpus {
        let memory = if gpu.integrated || hw.memory.is_unified_memory { "shared memory" } else { "VRAM" };
        row(
            &format!("gpu {}", gpu.index),
            format!("{} | {:.2} GB free of {:.2} GB {}", gpu.name, gpu.vram_available_gb, gpu.vram_total_gb, memory),
        );
    }
    if backends.is_empty() {
        // A GPU is there (NVML found it) but no GPU pack loaded: say so, or "slow" is all the user sees
        let note = if hw.gpus.is_empty() {
            "none, models run on the CPU".to_string()
        } else {
            "none loaded, models run on the CPU. Install the GPU pack for this GPU, or update its driver".to_string()
        };
        row("backend", note);
    }
    for (i, backend) in backends.iter().enumerate() {
        row(if i == 0 { "backend" } else { "" }, backend.clone());
    }
}
