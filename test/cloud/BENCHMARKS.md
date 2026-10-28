# Cross-platform runs

Each run has its folder in `runs/` with a `SUMMARY.md` and the raw logs. Models stay at or under 24B (Q4_K_M); settings are Auto unless a row says otherwise. Speeds are llama.cpp's own timings.

## Platforms
| Run | Date | OS | CPU / RAM | GPU | Build | Unit tests | Calls | Result |
|---|---|---|---|---|---|---|---|---|
| [3](runs/2026-10-27-run3-linux-cpu/SUMMARY.md) | 2026-10-27 | Ubuntu 24.04 | Xeon 2.8 GHz, 8 vCPU / 31 GiB | none | ✅ 19 min | 366 / 366 | 21 / 25, then Whisper fixed | ✅ after `95165aa` |
| [1](runs/2026-10-27-run1-linux-t4/) | 2026-10-27 | Ubuntu 22.04 | 8 vCPU / 52 GiB | T4 16 GB | ❌ link: ONNX Runtime needs glibc ≥ 2.38 | — | — | moved to 24.04 |
| 1 (again) | — | Ubuntu 24.04 | | T4 16 GB | | | | not run yet (Spot T4 reclaimed within minutes, 3 tries) |
| 2 | — | Ubuntu 24.04 | | L4 24 GB | | | | not run yet |
| [4a](runs/2026-10-28-run4a-linux-t4-release-cuda13/) | 2026-10-28 | Ubuntu 24.04, driver 580 (CUDA 13.0) | Xeon 2.3 GHz, 8 vCPU / 29 GiB | T4 16 GB | release v0.1.0 by `install.sh`, CUDA 13 pack | — | 0 / 29 | ❌ CUDA 13.4 code cannot run on a CUDA 13.0 driver for T4; installer fixed |
| [4b](runs/2026-10-28-run4b-linux-t4-release-cuda12/SUMMARY.md) | 2026-10-28 | same VM | same | T4 16 GB | release v0.1.0 by `install.sh`, CUDA 12 pack | — | 11 / 35 | ❌ server aborted on Kokoro CUDA (system cuDNN for CUDA 13); engine fix pending |
| [5](runs/2026-10-28-run5-linux-t4-regression/RESULTS.md) | 2026-10-28 | Ubuntu 24.04, driver 580 | Xeon 2.3 GHz, 8 vCPU / 29 GiB, then 4 vCPU / 15 GiB | T4 16 GB | release v0.1.0 (CUDA 12 pack), then `main` with two prompt fixes | — | 18 / 18 `run.py` tests | ✅ Cluaiz vs llama.cpp: generation equal; prompt equal after the fixes, up to 1.6x faster split |
| 4 | — | Windows Server | | T4 16 GB | | | | not run yet |

## Chat speed (generation tokens/s)
| Model (Q4_K_M) | Linux CPU 8 vCPU | Linux T4 | Linux L4 | Windows T4 | Owner's PC (RTX 3050 4 GB) |
|---|---|---|---|---|---|
| Qwen3-4B | 10.5 (CPU, 4096) | 64.6–68.0 (full GPU, 40960) | | | 32–42 (full GPU, 2048–2560) |
| Gemma-3 12B | 3.6 (CPU, 4096) | 14.4–24.6 (full GPU, 16896–17408) | | | |
| Mistral-Small-3.2 24B | 1.9 (CPU, 4096) | 8.0–11.2 (full GPU, 3072–4096) | | | |
| Gemma 4 31B (17.1 GB) | | 3.0–3.2 (planner: 41 / 60 blocks on GPU, 4096) | | | |
| Gemma 4 31B Q8_0 (30.4 GB) | | 1.3–1.4 (planner: 23 / 60 blocks on GPU, 4096) | | | |

## Voice and embeddings (seconds per call, after the first load)
| | Linux CPU | Linux T4 | Linux L4 | Windows T4 | Owner's PC |
|---|---|---|---|---|---|
| Kokoro TTS | 2.7–3.8 | | | | 0.4–0.9 (CUDA) |
| Whisper turbo STT | 3.4 | | | | 6.6–7.3 (CPU beside the chat) |
| bge-m3 embedding | 0.05–1.0 | | | | 0.1 |
| Qwen3-TTS 1.7B | 16–25 | | | | 17–19 |
