# Run 47: why `cluaiz bench` is slower than llama-bench on the Windows laptop

Date: 2026-11-23. Script: `test/cloud/remote/warm_tg_gap/run_warm_tg_gap.ps1` (rotated order, one load per
config per round, disk reads and free RAM per run from typeperf). Analysis of the Gemma parts: `analyze.py`
(GB read in the generation phase). Raw output of every run: `results/<part>/`.

## Setup

| | |
|---|---|
| Machine | run 20 / 42 laptop: Windows 11, Ryzen 7 7435HS (8 cores), 23.7 GB RAM, RTX 3050 Laptop 4 GB |
| Load from other programs | the owner's apps ran throughout: free RAM 5-14 GB, 1.3-1.6 GB of VRAM and up to 27% GPU use by others |
| llama.cpp | official `llama-bench` b12428 (`b4c0e00a4`), CUDA 13 build |
| Cluaiz | working tree at `746fd6d0` (fork `8676a916e`, b12388 + 32); `cargo build -p cmd` (debug, built by another agent at 09:37) and `cargo build -p cmd --release` (13 min 45 s, this run) |
| Models | Gemma 4 26B A4B IQ4_XS (14,205,004,928 bytes); Qwen3-1.7B IQ4_XS (bartowski, 1,175,689,632 bytes); granite-3.1-3b-a800m-instruct IQ4_XS (bartowski, 1,783,219,776 bytes, MoE, 32 blocks) |
| Flags | 8 threads, flash attention on, KV f16, 3 repetitions; split: `-ngl 99 -ncmoe N` / `--n-gpu-layers all --n-cpu-moe N`; CPU only: `-ngl 0 -dev none` / `--n-gpu-layers 0` |
| Home | isolated `CLUAIZ_HOME` under `%LOCALAPPDATA%\cluaiz-test`; no server, no port |

Swap configs (same exe, other ggml files beside it, hard links plus copies in test folders):
`L-forkcpu` = official llama-bench with Cluaiz's MSVC `ggml-cpu-*.dll`; `L-forkcuda` = official llama-bench with
Cluaiz's `ggml-cuda.dll`; `C-rel-clangcpu` / `C-clang-omp` = the Cluaiz release exe with the official clang-built
`ggml-cpu-*.dll` and `libomp.dll` (checked loaded with the process module list); `C-msvc` = the same exe with its
own MSVC CPU DLLs; `C-fix` = Cluaiz with CPU DLLs built by clang with `GGML_OPENMP=OFF` (a tried fix, reverted).
The ggml headers are identical between b12388 and b12428 (`git diff --stat b12388 b12428 -- ggml/include
ggml/src/ggml-impl.h ggml/src/ggml-backend-impl.h` prints nothing), so the swapped DLLs load and run.

## The build difference

| | official b12428 | Cluaiz |
|---|---|---|
| `ggml-cpu-haswell.dll`, `ggml-base.dll` | clang, imports `libomp.dll` (LLVM OpenMP) | MSVC, imports `VCOMP140.DLL` |
| `ggml-cuda.dll` | MSVC | MSVC |
| Where it is set | fork `.github/workflows/release.yml` windows-cpu job (`cmake/x64-windows-llvm.cmake`, `GGML_OPENMP_FETCH`), windows-cuda job (vcvarsall, MSVC) | `engine/llama/build.rs` (Visual Studio generator for all of llama.cpp) |

## Results

### Gemma 4 26B, run 42's flags (`-ncmoe 30`), parts `base` and `ab`

Not clean: the model's working set (about 13 GB) was at the edge of the free RAM, and most runs read 1-16 GB
from the SSD, some of it during generation (`analysis.md` in each part). tg128 medians of rounds 2+ (part `ab`,
5 rounds): L 17.14, L-forkcpu 15.75, L-forkcuda 17.10, C-debug 16.56, C-release 14.99, C-rel-clangcpu 15.62 t/s,
with single runs from 10.89 to 18.57. The 10% gap of run 42 can be neither confirmed nor explained from these
runs. (The `results.md` files of these two parts show 0: a parser bug in the script then, fixed before the
later parts; `analysis.md` reads the tool tables.)

### CPU only, Qwen3-1.7B IQ4_XS (part `cpu`, 4 rounds; RAM never limiting, disk reads under 0.6 GB)

| Config | pp512 median | tg128 median |
|---|---|---|
| L | 139.24 | 27.20 |
| L-forkcpu (MSVC CPU DLLs) | 121.18 | 28.09 |
| C-release (MSVC CPU DLLs) | 120.99 | 25.73 |
| C-rel-clangcpu (official clang + libomp CPU DLLs) | 137.34 | 27.64 |

The prompt gap (13%) moves with the CPU DLLs, not with the program: llama-bench with Cluaiz's DLLs is as slow
as Cluaiz, and Cluaiz with llama.cpp's DLLs is as fast as llama-bench. Generation is equal within the spread.
Repeated on a busier machine (part `fix-cpu`, 5 rounds): pp512 L 126.59, C-msvc 115.23, C-clang-omp 126.43.

### MoE split, granite 3B-A800M, all 32 blocks' experts in RAM (parts `moe-split`, `moe-split-pp`, `fix-split`)

| Part | L tg128 | MSVC CPU (L-forkcpu / C) | clang + libomp CPU (C) | clang, no OpenMP (C-fix) |
|---|---|---|---|---|
| `moe-split` (4 rounds) | 36.12 | 32.65 / 31.71 | 40.81 | |
| `moe-split-pp` (6 rounds, busier) | 41.07 | - / 35.19 | 35.99 | |
| `fix-split` (6 rounds) | 39.43 | - / 38.43 | 39.70 | 9.31 |

Split-mode generation: the MSVC loss ranges from 3% to 20% between parts; the GPU was shared with other
programs, so only the CPU-only prompt gap is settled. pp512 / pp2048 are equal in all split parts (for example
`fix-split`: L 1630.65 / 1728.48, C-msvc 1625.99 / 1835.88), and Cluaiz at `--batch-size 2048` against
llama-bench at `-b 512` changes nothing beyond the spread (`moe-split-pp`).

### The tried fix that was reverted

`engine/llama/build.rs` built the CPU variants a second time with upstream's toolchain file (clang 22, Ninja)
and `GGML_OPENMP=OFF`, so that no `libomp.dll` would have to ship. Prompt speed was fine, but split generation
fell to 9.31 t/s (4x slower, part `fix-split`): without OpenMP, ggml's CPU backend uses its own thread pool, and
Cluaiz never attaches one to its context (llama-bench and llama.cpp's common code do, `llama_attach_threadpool`,
fork `tools/llama-bench/llama-bench.cpp:2397-2416`, `common/common.cpp:1820-1849`), so each CPU graph starts its
own threads. The change was reverted and `target/release` got its MSVC CPU DLLs back. Two build problems met on
the way (both outside the fork): clang 22 stops on `_mm_prefetch(&x[ib] + ...)` in `ggml-cpu/arch/x86/quants.c`
(needs `-Wno-error=incompatible-pointer-types`; upstream CI pins clang 20), and clang 22 did not finish
`amx/mmq.cpp` of the sapphirerapids variant in 30 minutes.

### Other hypotheses

| Hypothesis | Evidence | Result |
|---|---|---|
| (a) debug Rust side | Gemma `ab`: C-debug 16.56 vs C-release 14.99 tg (noise-dominated); granite `moe-split`: C-debug 36.46, C-release 31.71 | not visible above the noise |
| (b) llama.cpp build | above | CPU modules: yes (prompt, CPU only); fork CUDA module: no (L-forkcuda not below L) |
| (c) defaults | n_batch 512 (Cluaiz, `engine/core/src/settings.rs:547`) vs 2048 (llama-bench): no pp difference (`moe-split-pp`); n_ubatch 512 both; mmap both (llama-bench AUTO resolves to mmap, fork `src/llama-model.cpp:1703`); flash attention on both; op_offload on both; 8 threads both | no difference found |
| (d) Rust per-token loop | `engine/llama/src/native/core.rs:275` bench: per token one `llama_decode` + `llama_synchronize`, as llama-bench; no per-token lines in the engine log | no |
| Cluaiz private memory | process sampling (`io/`): peak private 3.56 GB (`cluaiz bench`) vs 2.91 GB (llama-bench) on the Gemma split | 0.65 GB more; matters only when RAM is this tight |

### MoE cache and prompt speed (the run 45 question)

Not testable on this laptop: with `CLUAIZ_MOE_CACHE=on` and the planner's placement, no MoE cache was planned
(`moecache/plan-cache-on.log`: `n_gpu_layers = 23 | n_cpu_moe = 30`, no cache line; 1.28 GB of the 4 GB VRAM was
taken by other programs), as in runs 42 and 46. Without a cache, Cluaiz's prompt speed equals llama-bench here.

## What this shows

1. On Windows, Cluaiz's CPU backends are slower than llama.cpp's because they are built with MSVC and MSVC
   OpenMP; llama.cpp's release builds them with clang and LLVM OpenMP. Proven for CPU-only prompt speed (13%).
2. Upstream's exact setup (clang + LLVM OpenMP) keeps the speed: `C-clang-omp` equals llama-bench in every
   part. It needs `libomp.dll` beside the exe. The clang build without OpenMP is 4x slower in split generation
   unless Cluaiz attaches a thread pool.
3. The warm Gemma tg gap of run 42 could not be measured cleanly today (free RAM); its cause stays ?.

## Pending

| Item | Why |
|---|---|
| Owner decision: ship `libomp.dll` (clang + LLVM OpenMP, as upstream) or attach a ggml thread pool and build without OpenMP | the first needs release packaging, a license notice and the dev sync in `cmd/src/core/bootstrapper.rs` (it copies only ggml / llama / mtmd files, so `libomp.dll` would be missing); the second is new Rust code in the context setup, not measured |
| Gemma 4 26B warm tg gap | needs a run with more free RAM, or a machine where the model fits with room |
| pp gap on the L4 with the MoE cache (run 45) | the laptop's 4 GB GPU never gets a MoE cache; test on the L4 with `CLUAIZ_MOE_CACHE=off` against the default |
