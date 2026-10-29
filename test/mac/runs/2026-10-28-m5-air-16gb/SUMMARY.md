# Mac run 1: MacBook Air M5, 16 GB, release v0.1.0 (2026-10-28)

> **Summary.** The macOS release installed with its one command in about 2 minutes and served chat. At the same
> settings it matched plain llama.cpp (`llama-bench` built with Metal from the same llama.cpp commit): Qwen3-4B
> prompt 104–111 vs 105–110 t/s, generation 22.7–24.6 vs 22.1–22.5 t/s. But on this Mac the engine never used
> the Metal GPU: the hardware probe finds only NVIDIA GPUs, it read free RAM as 0.00–0.75 GB of 16 GB, and a
> request's `n_gpu_layers -1` was changed to 0. Auto placement refused even the 2.3 GB model (HTTP 500), and
> Gemma 3 12B was skipped. The numbers above are therefore the CPU with llama.cpp's op offload, not Metal.

## Machine
| | |
|---|---|
| Mac | MacBook Air (Mac17,3), Apple M5: 4 performance + 6 efficiency cores, 8-core GPU, Metal 4 |
| RAM | 16 GB unified |
| OS | macOS 26.6.2 (25G83) |
| Power | charger connected |
| SSD | 2 GB written at 5.1 GB/s |
| Install | `install.sh` from `main`, release v0.1.0 core `mac-arm64` (77.6 MB, checksum OK), `CLUAIZ_HOME=~/cluaiz-mactest/home`, port 8011 |
| Script | [`test/mac/mac_test.sh`](../../mac_test.sh), 41 minutes (35 of them model downloads) |

The tester's user name, host name and the Mac's serial number and IDs were removed from these logs.

## Results (Qwen3-4B Q4_K_M, tokens per second)

| Mode | Result | What the engine ran | Engine prompt / gen | llama-bench prompt / gen (same settings) |
|---|---|---|---|---|
| auto | **FAIL** (HTTP 500) | refused: "needs 2.91 GB more RAM than allowed" | - | - |
| gpu (`n_gpu_layers -1`) | PASS | `n_gpu_layers 0` | 104 / 22.7 | 110 / 22.1 |
| split | PASS | `n_gpu_layers 0` | 111 / 24.6 | 105 / 22.5 |
| cpu (`n_gpu_layers 0`) | PASS | `n_gpu_layers 0` | 88 / 17.2 | 82 / 16.3 |

Gemma 3 12B Q4_K_M (6.8 GB): skipped by `run.py`, because the engine reported less free RAM + VRAM than the file.
No run had every layer on the Metal GPU, so how far these numbers are from Metal speed is not known from this run;
`mac_test.sh` now also runs `llama-bench -ngl 99` on each model for that reference.

## Engine problems found
| # | Problem | Evidence | Status |
|---|---|---|---|
| 1 | The hardware probe asks NVML only, so an Apple GPU (and an AMD or Intel GPU on Vulkan) is not seen: `GPU: none`, auto placement never offloads. llama.cpp itself loaded the Metal backend (`Backends loaded; GPU offload available`). | `planner.log`, `server.log` line 8 | open |
| 2 | Free RAM read as 0.00 GB (once 0.75) of 16 GB. sysinfo 0.30 on macOS counts free + inactive + purgeable pages minus the compressor's pages, which reaches zero when macOS compresses memory, so the planner refused a 2.9 GB plan. | `planner.log` `RAM: free 0.00 of 16.00 GB` | open |
| 3 | A request's `n_gpu_layers -1` became 0 when no GPU was detected, against the rule that a user setting goes to llama.cpp as it is. | `planner.log` `User setting: ... n_gpu_layers 0` | fixed on `main` (planner) |
| 4 | A refused load answers HTTP 500 (server error) instead of a client error with the reason. | `run_qwen3-4b-gguf.log` | open |
| 5 | Install output: `curl: (56) Failure writing output` while finding the release, and a "Ghost Execution Detected" warning for the binary the installer just placed. Neither stopped the install. | `install.log` | open |

## Files
`mac_test.log` (the console), `machine.txt`, `install.log`, `installed_files.txt`, `pulls.log` and `pull_*.log`,
`llama_bench_build.log`, `server.log`, `planner.log` (every plan the server printed), `run_*.log` and
`RESULTS_*.md` (from `test/engine_regression/run.py`), `loaded_libs.txt` (empty: the server had already unloaded).
