# MoE model larger than VRAM + RAM, experts streamed from the SSD

## Generation (run 48, 2026-11-23)

Qwen3.6 35B A3B UD-IQ4_XS (18.2 GB file) on a GCP L4 with about 8 GB VRAM free and a 12 GB RAM limit (page cache
counted), page cache dropped before every run, one `serve` load per run, 4 threads, median of 3 rounds.

| Config | code | chat | list | long |
|---|---|---|---|---|
| **Cluaiz, all features on** (planner's MoE cache 3,603 MiB + expert prefetch, its default here) | **10.21** | **25.11** | **18.83** | **19.76** |
| llama.cpp at its best here: llama-server with its MoE cache `--moe-cache-mib 3600 --moe-cache-cpu-share 0.9` | 7.63 | 20.70 | 13.15 | 15.93 |
| llama.cpp at its defaults (`-ngl 99 -ncmoe 41`, mmap) | 6.17 | 10.97 | 8.73 | 9.09 |

| Ratio | code | chat | list | long |
|---|---|---|---|---|
| Cluaiz / llama.cpp with its MoE cache | 1.34x | 1.21x | 1.43x | 1.24x |
| Cluaiz / llama.cpp defaults | 1.65x | 2.29x | 2.16x | 2.17x |

The rounds of Cluaiz and llama.cpp with its cache do not overlap on any prompt. Only llama.cpp's cache size used
by Cluaiz's planner (3,600 MiB) was tried; other llama.cpp cache sizes were not.
Run: [run 48](../cloud/runs/2026-11-23-run48-l4-fair-baselines/SUMMARY.md).

## Prompt processing (run 75, 2026-11-25)

Same model, machine and limits, `cluaiz bench` against `llama-bench`, 3 rounds:

| Prompt | Cluaiz | llama-bench | Ratio |
|---|---|---|---|
| 512 tokens | 51.31-51.90 | 52.33-52.42 | equal |
| 2048 tokens, after pp512 | 94.34 (mean) | 103.72 (mean) | 0.91x |
| 2048 tokens alone | 91.74 (mean) | 97.19 (mean) | 0.94x |

The pp2048 gap is open: Cluaiz reads more from the disk in that phase (6.2 GB against about 5.3 GB).
Run: [run 75](../cloud/runs/2026-11-25-run75-pp2048-followup/SUMMARY.md).

## 4 GB laptop GPU (run 44, 2026-11-23)

Gemma 4 26B A4B IQ4_XS on an RTX 3050 Laptop 4 GB with low free RAM, experts read from the SSD, tg128 median of 3:
Cluaiz 15.97 t/s, llama-bench 15.73 t/s (equal). The expert prefetch is off by default on Windows; forced on in
this low-RAM case it was slower (10.23 t/s). Run: [run 44](../cloud/runs/2026-11-23-run44-laptop-prefetch-low-ram/SUMMARY.md).
