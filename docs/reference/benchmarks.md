<!-- name: Benchmarks
description: Cluaiz against official llama.cpp and Ollama on the same model files and placements: the short answer, then every table with its run. -->

# Benchmarks

Cluaiz and official llama.cpp's `llama-bench` on the same model file, the same placement and the same settings.
Speeds are llama.cpp's own timings in tokens per second (t/s); higher is better. Every row links to the run
folder with the commands, the settings and the raw logs. The list of all runs is
[`test/cloud/BENCHMARKS.md`](../../test/cloud/BENCHMARKS.md).

## Short answer

| | Cluaiz vs llama.cpp |
| :--- | :--- |
| Generation, GPU and GPU + RAM split | Equal (within the run-to-run noise): laptop RTX 3050 ([run 17](../../test/cloud/runs/2026-11-16-run17-laptop-same-settings/SUMMARY.md)), Tesla T4 ([run 5](../../test/cloud/runs/2026-10-28-run5-linux-t4-regression/RESULTS.md)), L4 ([run 9](../../test/cloud/runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md)) |
| Generation, CPU only | Equal, except Ternary-Bonsai Q2_0 on a 1 vCPU server: 13-20% slower ([run 16](../../test/cloud/runs/2026-11-16-run16-vps-ollama/SUMMARY.md)) |
| Prompt, model fully on the GPU | Equal on the laptop and the T4; llama.cpp up to 11% faster on the L4 |
| Prompt, model split across GPU and RAM | Equal with `cluaiz bench` (prompt 512, [run 17](../../test/cloud/runs/2026-11-16-run17-laptop-same-settings/SUMMARY.md)). Through the chat API, a prompt of about 535 tokens was up to 1.6x faster on the T4, because Cluaiz cuts it into two even chunks; on the L4 0.82x to 1.3x |
| Generation, MoE model larger than VRAM + RAM | Qwen3.6 35B A3B (18.2 GB file) on an L4 with 8 GB VRAM free and a 12 GB RAM limit, one `serve` load each, median of 3 rounds: Cluaiz (planner's MoE cache + expert prefetch, its default there) 10.21 / 25.11 / 18.83 / 19.76 t/s on four prompts; llama-server at its defaults 6.17 / 10.97 / 8.73 / 9.09 (**1.65-2.29x**); llama-server with its MoE cache `--moe-cache-mib 3600` 7.63 / 20.70 / 13.15 / 15.93 (**1.21-1.43x**). With the same cache and no prefetch both are equal (1.01-1.03x), so the gain over llama.cpp's cache is the prefetch ([run 48](../../test/cloud/runs/2026-11-23-run48-l4-fair-baselines/SUMMARY.md)) |
| Prompt, CPU only | Equal on a 1 vCPU server ([run 15](../../test/cloud/runs/2026-11-16-run15-vps-cpu/SUMMARY.md)); on an 8-thread Windows laptop about 5% behind since the CPU backends are built with clang and LLVM OpenMP (Qwen3-1.7B pp512: 129 vs 136 t/s, [run 54](../../test/cloud/runs/2026-11-23-run54-windows-llvm-cpu-backends/SUMMARY.md)); 6-26% behind before that change ([run 17](../../test/cloud/runs/2026-11-16-run17-laptop-same-settings/SUMMARY.md)) |

Against Ollama 0.40.16 with the same GGUF files: generation was equal where the placement was equal on an L4
([run 10](../../test/cloud/runs/2026-11-15-run10-ollama/SUMMARY.md)); on a 1 vCPU / 3.8 GB server without a GPU,
Ollama generated 14-23% slower than Cluaiz with the same settings and could not load Llama 3.2 3B
([run 16](../../test/cloud/runs/2026-11-16-run16-vps-ollama/SUMMARY.md)).

## A 26B model on a 4 GB laptop GPU (older run, before the expert prefetch)

For the current SSD streaming result (expert prefetch on) see the MoE row in the short answer above (run 48).

Gemma 4 26B A4B (MoE, IQ4_XS, 14.2 GB model file) on an RTX 3050 Laptop with 4 GB VRAM and 24 GB RAM: attention
on the GPU, experts in RAM, and the experts that do not fit RAM read from the memory-mapped model file on the disk
when used (measured without the expert prefetch, which is off by default on Windows). Measured 2026-11-16:

| Measure | Result |
| :--- | :--- |
| `cluaiz bench`, 128 tokens | **16.7 t/s** (official `llama-bench` with the same placement: about 6% faster) |
| Chat answers of about 400 tokens | **11-13 t/s** |
| First answer after loading | 2.8 t/s, until the experts are in RAM |
| First load while other apps held VRAM | the planner chose CPU only (9 t/s): known issue |

The less free RAM, the more is read from the disk and the slower it gets. Dense models that do not fit RAM + VRAM
are refused with the reason instead of crawling from the disk.
[Run 20](../../test/cloud/runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md).

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

Measured with the prompt fixes (commits `6120d017`, `4228752a`), which the current v0.1.0 includes; the build
tested first on this T4 did not, and showed llama.cpp 7–22% ahead in the cases marked below.

| Model | Full GPU (Cluaiz vs llama.cpp) | Split GPU + RAM (Cluaiz vs llama.cpp) | Split Speedup |
| :--- | :---: | :---: | :---: |
| **Qwen3 4B** | 1935 vs 1978 t/s (v0.1.0: 1758 vs 1882) | **593 vs 393 t/s** | **1.5x faster** |
| **Gemma 3 12B** | 690 vs 725 t/s (v0.1.0: 572 vs 649) | **228 vs 141 t/s** | **1.6x faster** |
| **Mistral Small 3.2 24B** | 372 vs 385 t/s (v0.1.0: 315 vs 340) | 154 vs 153 t/s (1085 tokens) | same |
| **Gemma 4 31B Q4** (planner split, 41/60) | - | **116 vs 117 t/s** (v0.1.0: 96 vs 117) | same |
| **Gemma 4 31B Q8** (planner split, 23/60) | - | **50 vs 51 t/s** (v0.1.0: 45 vs 51) | same |

CPU only, llama.cpp also without a GPU (`-dev none`): Qwen3 4B 27 vs 29, Gemma 3 12B 10 vs 10, Mistral 24B 5 vs 5,
Gemma 4 31B 4 vs 4 t/s.

### Less RAM, and a model bigger than RAM + VRAM (same VM with 15 GB RAM)

| Model | What happened | Cluaiz | llama.cpp |
| :--- | :--- | :---: | :---: |
| **Gemma 4 31B Q4** (17.1 GB) | Planner: 41 of 60 blocks on GPU | **2.3 t/s** gen, 112 t/s prompt | 2.3 t/s gen, 112 t/s prompt |
| **Gemma 4 31B Q8** (30.4 GB, dense) | Planner refuses: the closest split is 8 GB short of RAM, with a clear message | not loaded (by design) | - |
| | Forced by the user setting `n_gpu_layers 20`: weights read from the disk on every token | 0.008–0.014 t/s gen, 1.5–3.0 t/s prompt | not measured (test ended) |

> [!NOTE]
> **Reading the T4 numbers**: generation equals llama.cpp in every test (-3% to +8%; the T4's clock moves a few
> percent between runs). Prompt processing equals llama.cpp on full GPU after the two fixes and stays up to 1.6x
> faster when a model is split across GPU and RAM. A dense model larger than RAM + VRAM is refused by the planner
> instead of thrashing the disk; only MoE experts are read from the memory-mapped file on the disk when RAM is
> short (Gemma 4 26B above).
>
> **Full Report**: methodology, every setting, the exact llama-bench commands and all logs are in
> [`test/cloud/runs/2026-10-28-run5-linux-t4-regression/RESULTS.md`](../../test/cloud/runs/2026-10-28-run5-linux-t4-regression/RESULTS.md).


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
> **Full Report**: [`test/cloud/runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md`](../../test/cloud/runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md).

## Older results

The first laptop comparison (RTX 3050 4 GB, 2026-10-12, a build from before v0.1.0) is in
[`test/engine_regression/RESULTS.md`](../../test/engine_regression/RESULTS.md); run 17 above repeats it on a
later build with `cluaiz bench`.
