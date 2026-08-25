# Cluaiz LLM Optimization - Technical Master Guide

The `llm_optimization.json` configuration dictates low-level hardware interactions in Cluaiz—how neural network weights map to physical hardware (RAM/VRAM), how OS memory pages are managed, and how computation graphs are executed by the CPU and GPU backends.

---

## 1. Core Hardware & Memory Buffers

### A. VRAM Safety Buffer (`custom_vram_buffer_gb`)
- **Mechanism**: The Memory Governor monitors GPU VRAM and reserves a dynamic or static safety margin.
- **Default**: `Auto` (dynamic percentage with a 250MB minimum floor).
- **Custom Override**: Can specify direct gigabytes (e.g. `1.5` GB) to prevent OOM errors on heavily loaded systems.

### B. CPU RAM Safety Buffer (`custom_ram_buffer_gb`)
- **Mechanism**: Reserves System RAM headroom for the host operating system and background applications.
- **Default**: `Auto` (dynamic percentage with a 500MB minimum floor).
- **Custom Override**: Can specify direct gigabytes (e.g. `2.0` GB) to prevent OS freezing or paging stalls.

### C. Flash Attention (`flash_attention`)
- **Mechanism**: Uses tiled Flash Attention kernels to compute self-attention in fast on-chip SRAM instead of materializing $O(N^2)$ matrices in global VRAM.
- **Options**: `Auto`, `On`, `Off`.
- **Backend Behavior**: Supported in `llama.cpp` for modern GPUs. In ONNX Runtime, CUDA EP handles flash attention automatically when available.

### D. KV Cache Quantization (`kv_cache_quantization`)
- **Mechanism**: Controls the precision of Key-Value memory caches during multi-turn generation.
- **Options**:
  - `Auto`: Automatically selects Q4_0 with Flash Attention, or falls back to F16.
  - `Kv16`: Full precision (`GGML_TYPE_F16`).
  - `Kv8`: 8-bit precision (`GGML_TYPE_Q8_0`).
  - `Kv4`: 4-bit precision (`GGML_TYPE_Q4_0`).

---

## 2. Advanced Optimization & Superpowers

### A. Extreme MoE SSD Streaming (`extreme_moe_streaming`)
- **Mechanism**: Zero-RAM out-of-core streaming for Mixture-of-Experts (MoE) models directly from NVMe SSD using DMA staging and ping-pong buffers.
- **Options**: `On`, `Off`, `Auto`.

### B. Hybrid Memory Mode (`hybrid_memory`)
- **Mechanism**: Dynamically combines GPU VRAM and System RAM into a unified memory space, allowing models to exceed physical VRAM without crashing.
- **Options**: `Auto`, `On`, `Off`.

### C. Context Shifting (`context_shifting`)
- **Mechanism**: Rolling window token eviction that prunes the oldest conversation tokens when context saturation occurs, preserving prompt prefix and latest turns.
- **Modes**: `Off`, `Minimal` (5%), `Standard` (10%), `Aggressive` (25%), `Extreme` (50%), `Auto`.

### D. Memory Lock (`force_memory_lock` / `use_mlock`)
- **Mechanism**: Commands the OS via `mlock()` / `VirtualLock()` to lock weights in physical memory, preventing pagefile swapping.
- **Options**: `Auto`, `On`, `Off`.

### E. Speculative Decoding (`speculative_decoding`)
- **Mechanism**: Uses a secondary draft model or lookup mechanism to speculate future tokens, verified in batches by the target model.
- **Options**: `Auto`, `On`, `Off`.
