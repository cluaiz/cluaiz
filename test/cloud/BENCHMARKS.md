# Cross-platform runs

Each run has its folder in `runs/` with a `SUMMARY.md` and the raw logs. Models stay at or under 24B (Q4_K_M); settings are Auto unless a row says otherwise. Speeds are llama.cpp's own timings.

## Platforms
| Run | Date | OS | CPU / RAM | GPU | Build | Unit tests | Calls | Result |
|---|---|---|---|---|---|---|---|---|
| [3](runs/2026-10-27-run3-linux-cpu/SUMMARY.md) | 2026-10-27 | Ubuntu 24.04 | Xeon 2.8 GHz, 8 vCPU / 31 GiB | none | ✅ 19 min | 366 / 366 | 21 / 25, then Whisper fixed | ✅ after `95165aa` |
| [1](runs/2026-10-27-run1-linux-t4/) | 2026-10-27 | Ubuntu 22.04 | 8 vCPU / 52 GiB | T4 16 GB | ❌ link: ONNX Runtime needs glibc ≥ 2.38 | — | — | moved to 24.04 |
| 1 (again) | — | Ubuntu 24.04 | | T4 16 GB | | | | not run yet (Spot T4 reclaimed within minutes, 3 tries) |
| [8](runs/2026-11-14-run8-linux-l4/SUMMARY.md) | 2026-11-14 | Debian 13, driver 620.70 | Xeon 2.2 GHz, 8 vCPU / 31 GiB | L4 23 GB | release v0.1.0 (`14ae569f`) by `install.sh`, CUDA 13 packs | — | 25 / 25 | ✅ features and journeys pass; `bench` of Gemma 4 31B fails, see summary |
| [9](runs/2026-11-14-run9-linux-l4-regression/SUMMARY.md) | 2026-11-14 | same VM as run 8, CUDA toolkit 13.2 for llama-bench | same | L4 23 GB | release v0.1.0 (`14ae569f`) vs llama-bench (fork `8f9842cd1`) | — | 14 / 15 `run.py` tests | ✅ generation equal to llama.cpp in all; Gemma 4 31B Q4 auto picks GPU + RAM though full GPU fits (1.4x slower) |
| [10](runs/2026-11-15-run10-ollama/SUMMARY.md) | 2026-11-15 | run 8 L4 VM and run 7 CPU VM | same | L4 23 GB / none | release v0.1.0 vs Ollama 0.40.16, same GGUF files | — | 48 / 48 requests | ✅ generation equal where placement is equal; Ollama keeps Gemma 4 31B fully on the GPU (12.8 vs 8.9 t/s), uses 4.7 GB less VRAM on Gemma 3 12B, loads faster on the CPU |
| [11](runs/2026-11-15-run11-l4/SUMMARY.md) | 2026-11-15 | run 8 L4 VM | same | L4 23 GB | release v0.1.0 (`51d03856`, llama.cpp `17d1465f0`) vs llama-bench, Ollama 0.40.16, LM Studio | — | 12 / 16 placements, 22 / 24 features | ⚠️ three bugs (gpt-oss markup, client tool calls dropped, untagged pull took a draft); Cluaiz fastest generation of the three tools; Gemma 4 31B now full GPU (12.8 t/s) |
| [12](runs/2026-11-15-run12-l4/SUMMARY.md) | 2026-11-15 | same | same | L4 23 GB | release v0.1.0 (`c3b1b123`) | — | 6 / 6 placements, 24 / 24 features | ✅ the three bugs fixed; speed unchanged |
| [13](runs/2026-11-15-run13-moe-cache/SUMMARY.md) | 2026-11-15/16 | same | same | L4 23 GB, also limited to 8 and 4 GB | official llama.cpp `b54752d` only, MoE cache study (no Cluaiz build) | — | 69 server runs | ℹ️ with experts in RAM the MoE cache beats `--fit` at 8 GB (1.3–2.5x) and 4 GB (1.1–1.7x); not with a model larger than RAM |
| [15](runs/2026-11-16-run15-vps-cpu/SUMMARY.md) | 2026-11-16 | Ubuntu 24.04 | AMD EPYC 9354P, 1 vCPU / 3.8 GB | none | release v0.1.0 by `install.sh` vs official llama.cpp `b12428` | — | `bench`, 1 thread | ✅ prompt and generation equal on Q1_0 and Q4_K_M |
| [16](runs/2026-11-16-run16-vps-ollama/SUMMARY.md) | 2026-11-16 | same VPS | same | none | release v0.1.0 (`eb11d47d`) vs llama.cpp `b12428` and Ollama 0.40.16 | — | `bench`, 1 thread | ⚠️ Ternary-Bonsai Q2_0 13-20% slower than llama.cpp; Ollama generates 14-23% slower than Cluaiz |
| [17](runs/2026-11-16-run17-laptop-same-settings/SUMMARY.md) | 2026-11-16 | Windows 11 | Ryzen 7 7435HS, 8 threads | RTX 3050 4 GB | `main` at `eb11d47d` vs llama.cpp `b12428` | — | `bench`, gpu / split / cpu | ⚠️ GPU and split equal; CPU-only prompt 6-26% slower |
| [18](runs/2026-11-16-run18-vps-features/SUMMARY.md) | 2026-11-16 | same VPS as run 15 | same | none | release v0.1.0 (`eb11d47d`) | — | tools, thinking, vision, MCP, ingest, providers, agent | ⚠️ Llama 3.2 3B leaves `<\|python_tag\|>` after a tool result |
| [19](runs/2026-11-16-run19-vps-deep/SUMMARY.md) | 2026-11-16 | same VPS as run 15 | same | none | release v0.1.0 (`eb11d47d`), per-second resource log | — | 9 chat models, speech, load, agent, flows | ⚠️ 2 CLI timeouts on a busy 1 vCPU (agent probe, `flow run`) |
| [20](runs/2026-11-16-run20-laptop-moe-disk/SUMMARY.md) | 2026-11-16 | Windows 11 | Ryzen 7 7435HS, 23.7 GB | RTX 3050 4 GB | `main` at `16cd9a5a`, Gemma 4 26B A4B IQ4_XS | — | `bench`, `run` | ⚠️ 16.7 t/s bench (tg128), 11-13 t/s for 400-token chat answers, 2.8 t/s before the experts are in RAM; about 6% behind llama-bench with the same placement; planner chose CPU only (9 t/s) on the first load |
| [4a](runs/2026-10-28-run4a-linux-t4-release-cuda13/) | 2026-10-28 | Ubuntu 24.04, driver 580 (CUDA 13.0) | Xeon 2.3 GHz, 8 vCPU / 29 GiB | T4 16 GB | release v0.1.0 by `install.sh`, CUDA 13 pack | — | 0 / 29 | ❌ CUDA 13.4 code cannot run on a CUDA 13.0 driver for T4; installer fixed |
| [4b](runs/2026-10-28-run4b-linux-t4-release-cuda12/SUMMARY.md) | 2026-10-28 | same VM | same | T4 16 GB | release v0.1.0 by `install.sh`, CUDA 12 pack | — | 11 / 35 | ❌ server aborted on Kokoro CUDA (system cuDNN for CUDA 13); engine fix pending |
| [5](runs/2026-10-28-run5-linux-t4-regression/RESULTS.md) | 2026-10-28 | Ubuntu 24.04, driver 580 | Xeon 2.3 GHz, 8 vCPU / 29 GiB, then 4 vCPU / 15 GiB | T4 16 GB | release v0.1.0 (CUDA 12 pack), then `main` with two prompt fixes | — | 18 / 18 `run.py` tests | ✅ Cluaiz vs llama.cpp: generation equal; prompt equal after the fixes, up to 1.6x faster split |
| [7](runs/2026-11-14-run7-linux-cpu/SUMMARY.md) | 2026-11-14 | Ubuntu 24.04 | Xeon 2.8 GHz, 8 vCPU / 31 GiB | none | release v0.1.0 (`14ae569f`) by `install.sh` | — | 25 / 25 | ✅ features and journeys pass; `flow draft` needs more than 4096 context |
| 4 | — | Windows Server | | T4 16 GB | | | | not run yet |

## Chat speed (generation tokens/s)
| Model (Q4_K_M) | Linux CPU 8 vCPU | Linux T4 | Linux L4 | Windows T4 | Owner's PC (RTX 3050 4 GB) |
|---|---|---|---|---|---|
| Qwen3-4B | 10.5 (CPU, 4096); 13.4–14.0 run 7 | 64.6–68.0 (full GPU, 40960) | 75.7–85.1 (full GPU, 16384) | | 32–42 (full GPU, 2048–2560) |
| Gemma-3 12B | 3.6 (CPU, 4096); 4.4–4.9 run 7 | 14.4–24.6 (full GPU, 16896–17408) | 28.2–31.4 (full GPU, 16384) | | |
| Mistral-Small-3.2 24B | 1.9 (CPU, 4096); 2.6–2.8 run 7 | 8.0–11.2 (full GPU, 3072–4096) | 16.8–17.3 (full GPU, 16384) | | |
| Gemma 4 31B (17.1 GB) | | 3.0–3.2 (planner: 41 / 60 blocks on GPU, 4096) | `run` answers; `bench` fails (run 8); 12.6–12.8 (full GPU, 4096, run 11) | | |
| Gemma 4 31B Q8_0 (30.4 GB) | | 1.3–1.4 (planner: 23 / 60 blocks on GPU, 4096) | | | |
| Phi-4-mini (run 11) | | | 88.6 (full GPU, 16384); CPU only 6.6 | | |
| Qwen3.5-9B (run 11) | | | 43.8 (full GPU); CPU only 4.9 | | |
| Qwen3-30B-A3B-2507 (run 11) | | | 98.3–99.1 (full GPU, 16384); CPU only 7.9 | | |
| gpt-oss-20b MXFP4 (run 12) | | | 81.1–81.8 (full GPU, 16384); CPU only 8.8 | | |
| gpt-oss-120b MXFP4 (59 GiB, run 11) | | | 8.2 warm (all layers on GPU, experts of 25 in RAM, from SSD) | | |

## Voice and embeddings (seconds per call, after the first load)
| | Linux CPU | Linux T4 | Linux L4 | Windows T4 | Owner's PC |
|---|---|---|---|---|---|
| Kokoro TTS | 2.7–3.8; 2.5–3.7 run 7 | | 2.2–2.5 (CUDA from the second session) | | 0.4–0.9 (CUDA) |
| Whisper turbo STT | 3.4; 2.8–4.6 run 7 | | 3.6 | | 6.6–7.3 (CPU beside the chat) |
| bge-m3 embedding | 0.05–1.0; 0.05 run 7 | | 0.01 | | 0.1 |
| Qwen3-TTS 1.7B | 16–25; 13.4–13.7 run 7 | | 2.5–3.6 | | 17–19 |
