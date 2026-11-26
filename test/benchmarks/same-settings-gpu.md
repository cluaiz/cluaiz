# Same model, same placement, same settings as llama.cpp (GPU and GPU + RAM split)

Short answer: generation is equal on every GPU tested. Prompt fully on the GPU is equal on the laptop and the T4,
and llama.cpp is up to 11% faster on the L4. Split across GPU and RAM, the chat API's prompt is up to 1.6x faster
on the T4 (even prompt chunks) and equal with `cluaiz bench`.

## RTX 3050 Laptop 4 GB, Windows 11 (run 17, 2026-11-16)

`cluaiz bench` against official `llama-bench` `b12428`, 8 threads, prompt 512, generation 128, 3 runs. Prompt /
generation t/s (CPU-only rows are in [cpu.md](cpu.md)):

| Model | Mode | Cluaiz | llama.cpp |
|---|---|---|---|
| Ternary-Bonsai-2-4B Q2_0 (1.12 GiB) | auto (full GPU) | 1954 ± 96 / 68.9 ± 1.4 | 2028 ± 30 / 70.8 ± 1.3 |
| | gpu | 1925 ± 114 / 70.1 ± 1.5 | 2011 ± 31 / 69.8 ± 0.7 |
| | split (17 of 32) | 1310 ± 88 / 17.7 ± 4.6 | 1298 ± 81 / 15.7 ± 4.3 |
| Qwen3.5-4B Q4_K_M (2.54 GiB) | auto (split, 15 layers, Q8_0 KV) | 1139 ± 58 / 10.9 ± 3.3 | 1147 ± 92 / 10.1 ± 2.9 |
| | split (17 of 32) | 1225 ± 74 / 12.9 ± 3.9 | 1222 ± 84 / 12.5 ± 3.9 |
| OLMoE-1B-7B Q4_K_M (3.92 GiB) | auto, MoE cache off (`-ncmoe 9`) | 1183 ± 334 / 65.3 ± 1.1 | 1274 ± 180 / 61.7 ± 1.9 |
| | split (9 of 16) | 1266 ± 308 / 67.9 ± 0.2 | 1384 ± 182 / 67.3 ± 0.5 |

Run: [run 17](../cloud/runs/2026-11-16-run17-laptop-same-settings/SUMMARY.md).

## Tesla T4 16 GB, Ubuntu 24.04 (2026-10-28)

*Same test (`test/engine_regression/run.py`) on **Google Cloud `n1-standard-8`: Intel Xeon 2.3 GHz (4C/8T), 29 GB RAM,
NVIDIA Tesla T4 (16 GB VRAM), Ubuntu 24.04, driver 580**. Cluaiz is release v0.1.0 installed with `install.sh`
(CUDA 12 pack); llama.cpp is `llama-bench` built on the same machine from the same llama.cpp commit and given the
placement Cluaiz chose.*

### Generation speed (Tokens / Second)

| Model (Quantization) | Mode | Placement | Cluaiz | Upstream llama.cpp | Delta |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Qwen3 4B (Q4_K_M)** | auto / gpu | Full GPU | **65.5 t/s** | 62.3 t/s | +5% |
| | split | 18 of 36 blocks on GPU | **14.5 t/s** | 13.9 t/s | +4% |
| | cpu | CPU only | **7.7 t/s** | 7.9 t/s | same |
| **Gemma 3 12B (Q4_K_M)** | auto / gpu | Full GPU | **17.0 t/s** | 15.7 t/s | +8% |
| | split | 24 of 48 blocks on GPU | **5.5 t/s** | 5.4 t/s | same |
| | cpu | CPU only | **3.1 t/s** | 3.1 t/s | same |
| **Mistral Small 3.2 24B (Q4_K_M, 13.3 GB)** | auto / gpu | Full GPU | **8.0 t/s** | 8.2 t/s | same |
| | split | 20 of 40 blocks on GPU | **3.1 t/s** | 3.1 t/s | same |
| **Gemma 4 31B (Q4_K_M, 17.1 GB)** | auto | 41 of 60 blocks on GPU (planner) | **3.2 t/s** | 3.1 t/s | same |
| | split | 30 of 60 blocks on GPU | **2.3 t/s** | 2.3 t/s | same |
| | cpu | CPU only | **1.3 t/s** | 1.3 t/s | same |
| **Gemma 4 31B (Q8_0, 30.4 GB)** | auto | 23 of 60 blocks on GPU (planner) | **1.3 t/s** | 1.3 t/s | same |

### Prompt processing speed (~535 tokens prompt)

| Model | Full GPU (Cluaiz vs llama.cpp) | Split GPU + RAM (Cluaiz vs llama.cpp) | Split Speedup |
| :--- | :---: | :---: | :---: |
| **Qwen3 4B** | 1935 vs 1978 t/s | **593 vs 393 t/s** | **1.5x faster** |
| **Gemma 3 12B** | 690 vs 725 t/s | **228 vs 141 t/s** | **1.6x faster** |
| **Mistral Small 3.2 24B** | 372 vs 385 t/s | 154 vs 153 t/s (1085 tokens) | same |
| **Gemma 4 31B Q4** (planner split, 41/60) | - | **116 vs 117 t/s** | same |
| **Gemma 4 31B Q8** (planner split, 23/60) | - | **50 vs 51 t/s** | same |

CPU only, llama.cpp also without a GPU (`-dev none`): Qwen3 4B 27 vs 29, Gemma 3 12B 10 vs 10, Mistral 24B 5 vs 5,
Gemma 4 31B 4 vs 4 t/s.

## NVIDIA L4 23 GB, Debian 13 (2026-11-14)

*Same test on **Google Cloud `g2-standard-8`: Intel Xeon 2.2 GHz (4C/8T), 31 GB RAM, NVIDIA L4 (22.5 GB VRAM),
Debian 13, driver 620**. Cluaiz is release v0.1.0 (`14ae569f`) installed with `install.sh` (CUDA 13 packs);
llama.cpp is `llama-bench` built on the same machine from the same llama.cpp commit (`8f9842cd1`) and given the
placement Cluaiz chose. Prompt about 544 tokens.*

| Model (Quantization) | Mode | Placement | Cluaiz prompt / gen | llama.cpp prompt / gen | Generation |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Qwen3 4B (Q4_K_M)** | auto / gpu | Full GPU | 4735 / **82.6 t/s** | 4518 / 82.9 t/s | same |
| | split | half the blocks on GPU | **832** / 14.7 t/s | 625 / 15.3 t/s | -4% |
| **Gemma 3 12B (Q4_K_M)** | auto / gpu | Full GPU | 1497 / **30.4 t/s** | 1666 / 30.4 t/s | same |
| | split | half the blocks on GPU | **294** / 6.3 t/s | 240 / 6.3 t/s | same |
| **Mistral Small 3.2 24B (Q4_K_M)** | auto / gpu | Full GPU | 916 / **17.1 t/s** | 968 / 17.1 t/s | same |
| | split | half the blocks on GPU | 153 / 3.4 t/s | **204** / 3.4 t/s | same |
| **Gemma 4 31B (Q4_K_M, 17.1 GB)** | auto | 57 of 60 blocks on GPU (planner) | 443 / **8.8 t/s** | 450 / 8.8 t/s | same |
| | gpu | Full GPU (forced) | 586 / **12.4 t/s** | 648 / 12.4 t/s | same |
| **Gemma 4 31B (Q8_0, 30.4 GB)** | auto | GPU + RAM (planner) | 87 / **2.0 t/s** | 88 / 2.0 t/s | same |

> [!NOTE]
> **Reading the L4 numbers**: generation equals llama.cpp in every test. Fully on the GPU llama.cpp's prompt is up
> to 11% faster; split across GPU and RAM, Cluaiz's prompt is up to 1.3x faster, except Mistral (llama.cpp 1.3x).
> **Known issue**: for Gemma 4 31B Q4 the planner keeps 3 blocks in RAM although the whole model fits the L4;
> forced fully on the GPU it is 1.4x faster (12.4 vs 8.8 t/s).
>
> **Full Report**: [`test/cloud/runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md`](../cloud/runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md).
