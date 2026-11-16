# Run 15: CPU only on a 1 vCPU VPS, Cluaiz against official llama.cpp with the same settings (2026-11-16)

| | |
|---|---|
| Machine | VPS, 1 vCPU (AMD EPYC 9354P, AVX-512), 3.8 GB RAM, no GPU, Ubuntu 24.04 |
| Cluaiz | release v0.1.0 installed with `install.sh`, `cluaiz bench` |
| Reference | official llama.cpp (ggml-org) prebuilt `b12428` ubuntu-x64, `llama-bench` |
| Script | [`vps_cpu_same_settings.sh`](../../remote/vps_cpu_same_settings.sh) |
| Settings, both tools | 1 thread, prompt 512, generation 128, flash attention auto, 3 runs |

The `cpu` rows force the same placement and KV type in both tools (`-ngl 0`, F16 KV). In the `auto` rows,
Cluaiz's planner chose CPU only with a Q8_0 KV cache, and llama-bench was given the same KV type.

| Model | Mode | Cluaiz prompt / generation t/s | llama.cpp prompt / generation t/s |
|---|---|---|---|
| Bonsai 4B Q1_0 (0.53 GB) | cpu, F16 KV | 7.82 ± 0.09 / 6.49 ± 0.05 | 7.77 ± 0.03 / 6.56 ± 0.09 |
| Bonsai 4B Q1_0 | auto (Q8_0 KV) | 6.75 ± 0.16 / 5.79 ± 0.09 | 6.71 ± 0.14 / 5.99 ± 0.21 |
| Llama 3.2 3B Q4_K_M (1.88 GB) | cpu, F16 KV | 21.17 ± 0.23 / 8.16 ± 0.20 | 21.80 ± 0.66 / 7.60 ± 0.84 |
| Llama 3.2 3B Q4_K_M | auto (Q8_0 KV) | 17.10 ± 0.27 / 7.86 ± 0.65 | 16.71 ± 0.16 / 7.73 ± 0.06 |

With the same settings, both tools are within the run-to-run spread on this machine (prompt -3% to +2%,
generation -3% to +7%). The planner's Q8_0 KV cache on the CPU reads prompts 14-19% slower than F16 in
both tools; that is the KV type, not Cluaiz.

## Not measured

- Ternary-Bonsai-2-4B: `cluaiz pull …:Q2_0` in release v0.1.0 took `Ternary-Bonsai-2-4B-PQ2_0.gguf`, a PrismML
  type (142) that neither tool loads, so both rows are empty. Fixed on main (the tag must be a whole name
  part, as in llama.cpp's `-hf`); the fixed build pulls `Ternary-Bonsai-2-4B-Q2_0.gguf` (1.76 GB).
- GPU, split, SSD and MoE cache modes: this machine has no GPU.
- The CPU-only gap seen on the L4 (0.80-0.96x of llama-bench with 4 threads, run 11) did not show here with
  1 thread; more threads were not tested on this machine.
