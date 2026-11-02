# Mixture-of-Experts Models on Small GPUs

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

## Not built

There is no separate SSD streaming engine, expert prefetcher or expert heat tracking. An earlier
version of this page described one; none of it is in the code.

See also: [llm_optimization.md](llm_optimization.md).
