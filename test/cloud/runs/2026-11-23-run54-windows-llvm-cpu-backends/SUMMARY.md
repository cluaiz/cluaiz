# Run 54: Windows CPU backends built with clang + LLVM OpenMP, against MSVC and llama-bench

Date: 2026-11-23. The fix chosen after run 47 (`engine/llama/build.rs` `build_llvm_cpu_backends`): ggml-base and the
CPU variants built with upstream's `cmake/x64-windows-llvm.cmake` (clang 22.1.2, Ninja) and LLVM OpenMP,
`libomp.dll` beside the exe. Script: `test/cloud/remote/warm_tg_gap/run_warm_tg_gap.ps1`. Raw output: `results/` (every file kept; none is over 1 MB, so nothing was left out of git).

## Setup

| | |
|---|---|
| Machine | run 47 laptop: Windows 11, Ryzen 7 7435HS (8 cores), 23.7 GB RAM, RTX 3050 Laptop 4 GB |
| Load from other programs | the owner's apps and another agent's `cluaiz serve` ran throughout; free RAM 0.5-7.7 GB (parts `final-cpu`, `final-split` were the busiest, `final-cpu2`, `final-split2` quieter) |
| llama.cpp | official `llama-bench` b12428 (`b4c0e00a4`) |
| C-llvm | `CARGO_TARGET_DIR=target-llvm cargo build -p cmd --release --no-default-features` (the release workflow's core build, 29 min 08 s) on `f38411a8` + this change; `ggml-cuda.dll` copied from `target\release` for the split part (MSVC, as upstream; with `GGML_BACKEND_DL` it is the GPU pack) |
| C-msvc | `target\release\cluaiz.exe` with its MSVC-built ggml-base and CPU modules (same sources, run 47 build) |
| Imports checked | C-llvm `ggml-base.dll` and `ggml-cpu-haswell.dll` import `libomp.dll`; no `VCOMP140.DLL` |
| Models | Qwen3-1.7B IQ4_XS (1,175,689,632 bytes), granite-3.1-3b-a800m-instruct IQ4_XS (1,783,219,776 bytes) |
| Flags | 8 threads, flash attention on, KV f16, 3 repetitions, pp512 + tg128; CPU only `-ngl 0 -dev none` / `--n-gpu-layers 0`; split `-ngl 99 -ncmoe 32` / `--n-gpu-layers all --n-cpu-moe 32` |

## Results (median of rounds 2 and later; every round in the part's `results.md`)

| Part | Rounds | L pp512 / tg128 | C-llvm pp512 / tg128 | C-msvc pp512 / tg128 |
|---|---|---|---|---|
| CPU only, `final-cpu2` (quieter) | 8 | 136.16 / 26.15 | 129.45 / 25.03 | 117.39 / 24.54 |
| CPU only, `final-cpu` (busy) | 5 | 119.72 / 24.10 | 102.26 / 19.57 | 97.95 / 20.12 |
| MoE split, `final-split2` (quieter) | 8 | 1631.86 / 37.14 | 1515.50 / 34.95 | 1610.15 / 32.43 |
| MoE split, `final-split` (busy) | 6 | 1626.25 / 30.66 | 1557.73 / 34.13 | 1632.46 / 35.01 |

Run 47's swap test for comparison (CPU only, llama.cpp's own clang 20 DLLs inside Cluaiz): pp512 137.34 against
llama-bench 139.24 and MSVC 120.99.

## Clean-folder start through the dev sync

`target-llvm\release\cluaiz.exe dev-sync core` run with an isolated `CLUAIZ_HOME` from a folder whose
`target\release` is a junction to `target-llvm\release` (the sync reads `<cwd>\target\release` when the exe is
not under `target`). The synced `bin\` held `libomp.dll` next to `ggml-base.dll` and the CPU modules
(`results/devsync/`). From there `cluaiz bench` (CPU only, pp256 / tg32) exited 0 at 133.00 / 30.71 t/s and the
engine log shows `loaded CPU backend from ...\home\bin\ggml-cpu-haswell.dll`. With `libomp.dll` renamed away the
same exe did not start: exit code -1073741515 (0xC0000135, a DLL not found), so the sync and the package must
carry it.

## What this shows

1. CPU-only prompt speed: C-llvm 129.45 against MSVC 117.39 (+10%), against llama-bench 136.16 (-5%; MSVC was
   -14%). Generation equal within the spread in all three.
2. MoE split generation in the quieter part: C-llvm 34.95, MSVC 32.43 (+8%), llama-bench 37.14; the busy part
   shows no order. Prompt speed in split mode is equal within the spread (GPU-bound with op offload).
3. Not full parity: llama.cpp's own clang 20 DLLs reached 137.34 in run 47 where this clang 22 build reaches
   129.45. Candidates, not measured: compiler version (22 vs 20), LLVM OpenMP version (22 vs 20.1.8), the
   machine load of this run.

## Pending

| Item | Why |
|---|---|
| The remaining 5% CPU-only prompt gap | build with clang 20 (upstream's CI version) and compare on the same machine |
| A quiet-machine rerun | most parts had other programs on the CPU and GPU |
| The release workflow on GitHub | `CLUAIZ_REQUIRE_LLVM_CPU=1` and the runner's LLVM (version, `libomp.dll` present) were not run in CI yet |
| Gemma 4 26B warm tg gap (run 42) | still needs free RAM to measure (run 47) |
