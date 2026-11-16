# Run 17: laptop, Cluaiz against official llama.cpp with the same settings (2026-11-16)

| | |
|---|---|
| Machine | Laptop, AMD Ryzen 7 7435HS (8 cores), RTX 3050 4 GB (about 2.1 GB free during the run), Windows 11 |
| Cluaiz | local release build of main at `eb11d47d` (the v0.1.0 tag), `cluaiz bench` |
| Reference | official llama.cpp (ggml-org) prebuilt `b12428`, win CUDA 13.5, `llama-bench` |
| Records | [Archived commands, settings and results](results/); the original runner script is not included in this archive |
| Settings, both tools | 8 threads, prompt 512, generation 128, flash attention auto, 3 runs |

`auto`: Cluaiz's planner picks; llama-bench gets the `n_gpu_layers`, `n_cpu_moe` and KV type the plan printed.
`gpu`, `split` (half of the blocks + 1 on the GPU) and `cpu` are forced in both tools with an F16 KV cache.

Prompt / generation t/s:

| Model | Mode | Cluaiz | llama.cpp |
|---|---|---|---|
| Ternary-Bonsai-2-4B Q2_0 (1.12 GiB) | auto (full GPU) | 1954 ± 96 / 68.9 ± 1.4 | 2028 ± 30 / 70.8 ± 1.3 |
| | gpu | 1925 ± 114 / 70.1 ± 1.5 | 2011 ± 31 / 69.8 ± 0.7 |
| | split (17 of 32) | 1310 ± 88 / 17.7 ± 4.6 | 1298 ± 81 / 15.7 ± 4.3 |
| | cpu | 22.7 ± 0.1 / 14.2 ± 0.3 | 24.2 ± 0.1 / 14.4 ± 0.2 |
| Qwen3.5-4B Q4_K_M (2.54 GiB) | auto (split, 15 layers, Q8_0 KV) | 1139 ± 58 / 10.9 ± 3.3 | 1147 ± 92 / 10.1 ± 2.9 |
| | split (17 of 32) | 1225 ± 74 / 12.9 ± 3.9 | 1222 ± 84 / 12.5 ± 3.9 |
| | cpu | 51.6 ± 0.6 / 11.2 ± 0.2 | 59.7 ± 1.5 / 10.0 ± 0.4 |
| OLMoE-1B-7B Q4_K_M (3.92 GiB) | auto, MoE cache off (`-ncmoe 9`) | 1183 ± 334 / 65.3 ± 1.1 | 1274 ± 180 / 61.7 ± 1.9 |
| | split (9 of 16) | 1266 ± 308 / 67.9 ± 0.2 | 1384 ± 182 / 67.3 ± 0.5 |
| | cpu | 96.8 ± 3.3 / 42.7 ± 3.2 | 130.3 ± 2.6 / 40.0 ± 1.3 |

The `cpu` llama.cpp rows use `-ngl 0 -dev none`. With `-ngl 0` alone llama.cpp still sends large prompt batches
to the GPU (836-1052 t/s prompt here); Cluaiz's `--n-gpu-layers 0` keeps everything on the CPU, so `-dev none`
is the same setting.

## MoE GPU cache (OLMoE)

| | Plan | Prompt / generation t/s |
|---|---|---|
| Cluaiz auto, MoE cache on | `-ngl -1 -ncmoe 16`, cache 1.51 GB, CPU share 0.8 | 1072 ± 315 / 77.4 ± 4.5 |
| Cluaiz auto, MoE cache off | `-ngl -1 -ncmoe 9` | 1183 ± 334 / 65.3 ± 1.1 |
| llama.cpp `-ngl 99 -ncmoe 16`, no cache | | 1071 ± 35 / 46.7 ± 3.4 |

The cache gives 1.18x generation over the best placement without it, and 1.66x over the same placement
without it. llama-bench has no MoE cache flag, so llama.cpp with the cache was not measured here (run 13
measured it with llama-server on the L4).

## Findings

- GPU, split and auto: same speed within the run-to-run spread (prompt -9% to +1%, generation -3% to +13%).
- CPU only, 8 threads: Cluaiz reads prompts 6-26% slower than llama.cpp (Bonsai -6%, Qwen -14%, OLMoE -26%);
  generation is equal or faster (-1% to +12%). The same prompt gap showed on the L4 with 4 threads (run 11)
  and not on the 1 vCPU VPS (run 15). Not explained yet.
- `--n-gpu-layers 0` in Cluaiz does not use the GPU for prompt batches, while llama.cpp's `-ngl 0` does.
- Prompt length matters for a split model. Qwen3.5-4B, 17 of 32 layers on the GPU, prompt only:
  512 tokens 1032 ± 52 (Cluaiz) / 1106 ± 65 (llama.cpp); 530 tokens 625 ± 16 / 629 ± 20. Both bench tools cut 530
  into 512 + 18, and the last 18 tokens run on the CPU. The 1.4-2.2x split prompt gain in the README (2026-10-12)
  was measured through Cluaiz's chat API, which cuts 530 into 265 + 265, against llama-bench; `cluaiz bench` does
  not use that path, so it is not in this table.
- OLMoE prompt speed varies a lot between runs in both tools (±15-30%); other programs were running.

## Not measured

- Ollama: not enough free disk on the laptop (2.7 GB); measured on the VPS (run 16).
- Full GPU for Qwen3.5-4B and OLMoE: they do not fit in the free VRAM.
- gpt-oss-20b and larger models: not enough free RAM.
