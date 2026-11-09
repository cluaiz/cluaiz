# Hardware Troubleshooting & VRAM Management

This document outlines common hardware bottlenecks, performance limits, and safe mitigation configurations when running the cluaiz Inference Engine.

---

## The GPU is not used

When a GPU pack cannot load (it is not installed, or the driver is missing or too old), llama.cpp skips it and
models run on the CPU. Nothing fails; it is only slower.

* **Check:** `cluaiz status`. The `backend` line names each GPU backend that loaded, for example
  `CUDA0: NVIDIA GeForce RTX 3050 Laptop GPU`. `none loaded` next to a listed GPU means the pack for it did not load.
* **Fix:** install again so the installer adds the pack for this GPU (`CLUAIZ_GPU=cuda` or `CLUAIZ_GPU=vulkan` to
  choose it), and update the GPU driver.

## The port is already in use

`cluaiz serve` stops with "port 8000 is already in use by another program" when another program listens on that
port. Stop that program, or choose another port with `cluaiz permission port <number>` (or set `CLUAIZ_PORT` for one
run).

---

## 1. PCIe Bandwidth Bottlenecks (PCIe Spills)

When the model size exceeds the physical VRAM limit of your GPU, the Operating System will allocate "Shared GPU Memory" inside System RAM. The GPU will continue computing, but it must pull weights over the PCIe bus continuously.

* **Symptom:** Token throughput plummets from 30+ TPS to under 2 TPS. The GPU usage shows 100%, but power draw remains extremely low.
* **Mitigation:**
  * Lower the GPU layers: `cluaiz config set optimization.n_gpu_layers 16` (instead of full offload `-1`), or `cluaiz run <model> -ngl 16` for one run.
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
