# Hardware Troubleshooting & VRAM Management

This document outlines common hardware bottlenecks, performance limits, and safe mitigation configurations when running the cluaiz Inference Engine.

---

## 1. PCIe Bandwidth Bottlenecks (PCIe Spills)

When the model size exceeds the physical VRAM limit of your GPU, the Operating System will allocate "Shared GPU Memory" inside System RAM. The GPU will continue computing, but it must pull weights over the PCIe bus continuously.

* **Symptom:** Token throughput plummets from 30+ TPS to under 2 TPS. The GPU usage shows 100%, but power draw remains extremely low.
* **Mitigation:**
  * Adjust `n_gpu_layers` inside `system_booster.json` to a custom hybrid value (e.g. `16` layers instead of full offload `-1`).
  * Check the model's footprint against available VRAM using `cluaiz status`.

---

## 2. Laptop Power Throttling (10W Computing Bounds)

On battery power or thermal limits, laptop CPU and GPU power states are capped, sometimes dropping core packages to less than 10W total power allocation.

* **Symptom:** Steady token throughput suddenly drops to 1-2 TPS after a few seconds of execution, or when battery level drops below 20%.
* **Mitigation:**
  * Enable `"force_memory_lock"` to prevent paging.
  * Reduce CPU threads count to exactly match physical cores (`cpu_thread_limit`).
  * Use a lower context shift strategy (`"standard"` or `"minimal"` instead of `"aggressive"`).

---

## 3. Out of Memory (OOM) Crashes

If a model execution requests more memory than physical memory (RAM + swap / VRAM), the Operating System will kill the engine daemon (`cluaizdb` or `cluaiz serve`).

* **Mitigation:**
  * Configure `vram_limit_gb` in your hardware profile to reserve at least 1.5GB for host OS display operations.
  * Activate KV-Cache Quantization (`kv-quant: "kv8"` or `"kv4"`) to reduce active memory footprint during long-context tasks.
