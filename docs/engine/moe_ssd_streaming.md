# Mixture-of-Experts Models on Small GPUs

![For the first N blocks the routed experts stay in RAM and attention, shared weights and KV go on the GPU, with a MoE GPU cache of the most used experts; later blocks are fully on the GPU; N is chosen from free VRAM](../assets/engine/moe-placement.png)

A Mixture-of-Experts (MoE) model can be larger than the GPU. The Engine runs it the same way
llama.cpp does with `--n-cpu-moe`:

- The routed experts of the first N blocks stay in RAM (CPU buffers). Everything else of those
  blocks (attention, shared weights, KV cache) and all later blocks go on the GPU.
- The planner chooses N from free VRAM; `optimization.n_cpu_moe` in `settings.json` sets it by
  hand.
- With `op_offload` (llama.cpp default when any layer is on the GPU), large prompt batches copy
  the RAM-held weights to the GPU to compute there.
- The GGUF file is memory-mapped (`use_mmap`), so weights that do not fit in RAM are read from
  disk by the operating system.

Code: `engine/llama/src/moe_override.rs`, `engine/plan/`.

## MoE GPU cache

When the experts stay in RAM and the model fits in RAM, the planner also turns on llama.cpp's MoE GPU
cache (`moe_cache_size`, `moe_cache_cpu_share`; llama.cpp #29887 and #30966). The cache keeps the most
used experts in VRAM for generation, and the CPU computes a share of the misses. On an L4 limited to
8 GB this made generation 1.3-2.2x faster than llama.cpp's `--fit` placement without a cache, and
1.1-1.5x faster at 4 GB (experts mmap'd, as the Engine loads them). A model larger than RAM gets no cache: there the disk limits generation, and
the cache was slower than the placement without it. `CLUAIZ_MOE_CACHE=off` turns the cache off.

Rules and numbers: [engine/plan/README.md](../../engine/plan/README.md#moe-gpu-cache), test record:
[run 13](../../test/cloud/runs/2026-11-15-run13-moe-cache/SUMMARY.md).

## Not built

There is no separate SSD streaming engine, expert prefetcher or expert heat tracking in the current
code. An earlier version (August 2026, in the `interface-engines` and `inference-engine` crates) had
one; it was not ported when those crates were replaced on 2026-10-11 (commits `696db9f1`, `b2350380`).
Official llama.cpp has no SSD expert streaming either: experts that do not fit in RAM are read
through the memory-mapped file.

## Under development

Planned for a later release (0.1.1 or 0.2.0), not in v0.1.0:

- **SSD prefetch / direct I/O for MoE experts** read from the disk, inside llama.cpp.
- **Speculative decoding:** MTP heads first (`mtp-*.gguf` files and models with built-in MTP layers),
  then draft models such as DFlash as an option. The Engine does not use either today.

See also: [llm_optimization.md](llm_optimization.md).
