# Cluaiz LLM Optimization - Technical Master Guide

The `llm_optimization.json` configuration dictates low-level hardware interactions in Cluaiz—how neural network weights map to physical hardware (RAM/VRAM), how OS memory pages are managed, and how computation graphs are executed across CPU and GPU backends.

---

## 1. Compute Device & GPU Offload (`n_gpu_layers`)

Determines how the Transformer blocks (layers) of a model are split between System RAM (CPU) and Video RAM (GPU).

### A. Full GPU Offload (`-1`)
* **Mechanism:** Attempts to offload all transformer layers into GPU memory (CUDA, ROCm, or Metal).
* **Boundary:** If model size exceeds physical dedicated VRAM, the operating system allocates Shared GPU Memory over the PCIe bus, degrading token generation throughput.
* **Guideline:** Only use Full Offload when `Total Model Weights + KV Cache + Workspace < Free Dedicated VRAM`.

### B. CPU Only (`0`)
* **Mechanism:** Completely bypasses dedicated accelerators. Weights are memory-mapped (`mmap`) into System RAM. Matrix arithmetic executes on CPU SIMD units (AVX2, AVX-512, ARM NEON).
* **Advantage:** Eliminates PCIe bus transfer overhead when model weights cannot fit in GPU VRAM.

### C. Tiered Hybrid Placement (`1` to `N`)
* **Mechanism:** Pins $N$ layers into GPU VRAM while keeping remaining layers in System RAM. During forward evaluation, intermediate activation tensors cross the PCIe bus once per layer boundary.
* **Advantage:** Balances compute throughput on memory-constrained systems without triggering OS page swapping.

---

## 2. Core Hardware Safety Buffers

### A. VRAM Safety Buffer (`custom_vram_buffer_gb`)
* **Mechanism:** The Memory Governor monitors GPU VRAM and enforces a dedicated safety headroom for desktop window compositing (DWM, X11, Wayland) and graphics drivers.
* **Default:** `Auto` (dynamic percentage with a 250MB minimum floor).
* **Override:** Explicit gigabytes (e.g. `1.0` GB) for systems running background display tasks.

### B. System RAM Safety Buffer (`custom_ram_buffer_gb`)
* **Mechanism:** Reserves Host RAM headroom to prevent OS memory exhaustion, disk pagefile thrashing, and system freezes.
* **Default:** `Auto` (15% of total system RAM, clamped between 3.5 GB and 6.0 GB).
* **Override:** Explicit gigabytes (e.g. `4.0` GB) to guarantee host application stability.

---

## 3. Attention & Cache Optimizations

### A. Flash Attention (`flash_attention`)
* **Mechanism:** Computes scaled dot-product attention via tiled blocks inside fast on-chip SRAM instead of materializing full $O(N^2)$ attention matrices in global VRAM.
* **Options:** `Auto`, `On`, `Off`.
* **Backend Behavior:** Supported in `cluaiz-llama` for modern GPUs. In ONNX Runtime, CUDA execution providers dispatch optimized flash kernels automatically when hardware supports it.

### B. KV Cache Quantization (`kv_cache_quantization`)
* **Mechanism:** Controls the precision of Key-Value memory caches across conversation turns to scale context length without linear VRAM growth.
* **Options:**
  - `Auto`: Automatically selects Q4_0 with Flash Attention, or falls back to F16.
  - `Kv16`: Full precision (`GGML_TYPE_F16`).
  - `Kv8`: 8-bit precision (`GGML_TYPE_Q8_0`), halving cache memory footprint with minimal perplexity deviation.
  - `Kv4`: 4-bit precision (`GGML_TYPE_Q4_0`), reducing cache memory footprint by 75% for long-context sequences.

### C. Context Shifting (`context_shifting`)
* **Mechanism:** Rolling window token eviction that prunes older intermediate conversation turns when context window saturation approaches, while preserving the initial system prompt and recent turns.
* **Modes:** `Off`, `Minimal` (5%), `Standard` (10%), `Aggressive` (25%), `Extreme` (50%), `Auto`.

---

## 4. Advanced Execution Features

### A. Native MoE SSD Streaming (`extreme_moe_streaming`)
* **Mechanism:** Out-of-core expert streaming for Mixture-of-Experts (MoE) architectures directly from storage via unbuffered direct I/O and slot caching, keeping only active routed experts and dense attention in memory.
* **Options:** `Auto`, `On`, `Off`.

### B. Memory Lock (`force_memory_lock` / `use_mlock`)
* **Mechanism:** Commands the operating system via `mlock()` (POSIX) or `VirtualLock()` (Windows) to lock model weight pages into physical memory, preventing the OS kernel from paging them to disk during idle periods.
* **Options:** `Auto`, `On`, `Off`.
* **Safety Floor:** Only activate when free system physical RAM exceeds total model footprint with adequate OS headroom.

### C. Speculative Decoding (`speculative_decoding`)
* **Mechanism:** Uses a fast draft model to generate candidate tokens that are validated in parallel by the target model in a single batched verification pass.
* **Options:** `Auto`, `On`, `Off`.

### D. Think Mode / Extended Reasoning (`think_mode`)
* **Mechanism:** Directs compatible reasoning models to execute structured chain-of-thought evaluations before emitting visible response tokens.
* **Options:** `Auto`, `On`, `Off`.

### E. Structured Output Enforcement (`enforce_json`)
* **Mechanism:** Applies vocabulary logit masking at each sampling step to ensure generated tokens strictly conform to valid JSON schemas.
* **Options:** `true`, `false`.

---

## 5. LLaMA vs ONNX Architectural Reality

Different interface engines handle optimization configurations according to their underlying runtime designs:

* **`cluaiz-llama` (Dynamic Engine):** Loads GGUF model files and dynamically applies layer allocation, runtime quantization, Flash Attention, and streaming configurations directly to GGML compute graphs.
* **`cluaiz-onnx` (Static Graph Engine):** Loads pre-compiled computational graphs.
  - **`n_gpu_layers`**: Respected and mapped to Execution Providers (CPU vs CUDA/DirectML).
  - **Flash Attention**: Executed automatically by the CUDA Execution Provider when hardware supports it.
  - **Quantization**: Fixed by the pre-quantized ONNX model artifact (e.g. `model-int4.onnx`). KV caches are managed natively by the graph execution provider for deterministic throughput.
