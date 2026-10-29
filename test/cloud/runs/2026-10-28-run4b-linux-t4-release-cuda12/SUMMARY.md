# Run 4 — Linux, Tesla T4, release v0.1.0 by one-command install (2026-10-28)

> **Summary.** First run of the release installed like a user would, with its one command, on a GPU. The
> install took 17–45 s. The CUDA 13 pack crashed the server on the T4 at the first chat (4a); the installer now
> reads the driver's CUDA version and gives the T4 the CUDA 12 pack. On the CUDA 12 pack the 4B, 12B and 24B
> models all ran fully on the GPU (4B 65 t/s, 12B 22–24 t/s, 24B 11 t/s); Kokoro, Whisper, bge-m3 and
> Qwen3-TTS were correct in the first round. Then Kokoro on the GPU aborted the server: the VM image's cuDNN
> (built for CUDA 13) was loaded with the CUDA 12 provider. Two Linux problems in the engine were found
> (below); the fix is pending.

## Machine
| | |
|---|---|
| VM | Google Cloud `n1-standard-8` + 1× Tesla T4 (15 GB), Spot, us-central1-a (preempted once after 5 min, restarted) |
| CPU / RAM | Intel Xeon @ 2.30 GHz, 8 vCPU / 29 GiB |
| Driver | 580.178.04, `nvidia-smi` CUDA Version 13.0 |
| OS | Ubuntu 24.04.5 (Deep Learning VM image `common-cu129-ubuntu-2404-nvidia-580`: also has CUDA 12.9 and cuDNN 9.13 for CUDA 13 in the system library path) |
| Install | `install.sh` from `main`, release v0.1.0 (`7700bba`), `CLUAIZ_HOME=~/ch`, port 8011 |

## 4a — CUDA 13 pack (`../2026-10-28-run4a-linux-t4-release-cuda13/`)
Installer (old rule: driver 580+ → CUDA 13) took core + `llama-cuda13` + `cudart13` + `onnx-cuda13`, 1.1 GB, 17 s. All 7 pulls exit 0. The first chat aborted the server:
`cudaFuncGetAttributes(&attr, kernel)` → `ggml-cuda.cu:109: CUDA error`; 29 of 29 checks failed.

**Cause:** llama.cpp's default CUDA architectures give T4 (7.5) only PTX (`75-virtual`); the driver must compile it, and a driver for CUDA 13.0 cannot compile PTX from toolkit 13.4. **Fix:** installers pick the pack from the driver's CUDA version and the GPU (ready-made code for 8.6 / 8.9 / 12.x); T4 on CUDA 13.0 now gets CUDA 12 (checked on this VM with the final `install.sh`).

## 4b — CUDA 12 pack (this folder)
Installer took core + `llama-cuda12` + `cudart12` + `onnx-cuda12`, 45 s. Server log: `Backends loaded; GPU offload available`.

| Call | Result | Placement | Context | Prompt t/s | Gen t/s | Time |
|---|---|---|---|---|---|---|
| Qwen3-4B short (first call) | ok | Full GPU | 40960 | 1.6 | 4.4 | 83 s |
| Qwen3-4B ~1.2k tokens | ok | Full GPU | 40960 | 1916 | 64.6 | 1.8 s |
| Gemma-3 12B short (first call) | ok | Full GPU | 17152 | 0.5 | 24.0 | 95 s |
| Gemma-3 12B ~1.2k tokens | ok | Full GPU | 17152 | 697 | 22.1 | 4.7 s |
| Mistral-Small-3.2 24B short | ok | Full GPU | 4096 | 384 | 11.2 | 94 s (load) |
| Mistral-Small-3.2 24B ~1.2k tokens | ok | Full GPU | 4096 | 370 | 10.9 | 10.5 s |
| Kokoro (ONNX) | ok (CPU: the 24B filled the GPU) | | | | | 3.3 s |
| Whisper turbo int8 (ONNX) | ok, `"Rivers carry water to the sea."` (CUDA ran out of memory, fell back to CPU) | | | | | 26 s |
| bge-m3 (GGUF) | ok | | | | | 4.6 s |
| Qwen3-TTS 1.7B (GGUF) | ok | | | | | 28 s |
| mixed round 0 chat (4B) | ok, 647 / 66.9 t/s | Full GPU | | | | 16.5 s |
| mixed round 0 Kokoro on the GPU | **server aborted**: `Invalid handle. Cannot load symbol cublasLtCreate` | | | | | |

11 of 35 passed; the other 24 found the server gone.

**First-call slowness (83–95 s, prompt 0.5–1.6 t/s):** the CUDA 12 pack also has only PTX for 7.5, so the driver compiles llama.cpp's kernels on first use. Later calls are full speed.

## Engine problems found (not fixed yet)
| # | Problem | Evidence |
|---|---|---|
| 1 | On Linux the engine accepts any `libcudnn.so.9` in the system path as "available" and skips its own download, without checking that it was built for the provider's CUDA major. Here the system cuDNN is the CUDA 13 build; with the CUDA 12 provider it looked for `libcublasLt.so.13` and aborted the whole process. Machines with a CUDA toolkit or cuDNN installed (common for ML users) can hit this. | `dpkg`: `libcudnn9-cuda-13 9.13.0.50`; server log last line |
| 2 | On Linux nothing makes the engine's own drivers folder visible to the loader (`register_drivers_dir_in_path` only acts on Windows), and the Linux provider also needs `libcurand.so.10`, which neither the pack nor the download list has. On a clean machine ONNX on CUDA would not load its libraries; here the image's CUDA 12.9 libraries hid that. | `readelf -d libonnxruntime_providers_cuda.so`; `ldconfig -p` |
| 3 | A CUDA failure inside llama.cpp (`ggml_abort`) or cuDNN ends the whole server instead of failing the one request. | 4a and 4b server logs |

**Update 2026-10-29:** 1 and 2 are fixed in `engine/onnx/src/cudnn_driver.rs`. cuDNN now counts only from the
engine's drivers folder, installed for the provider's CUDA major; cuRAND is in the download list for Linux; and on
Linux the runtime libraries are loaded by full path before the first CUDA session. Checked on a Google Cloud VM with
the same image (system cuDNN 9.13 for CUDA 13) and the v0.1.0 CUDA 12 provider: the engine downloaded cuDNN 9.8 for
CUDA 12, and every cuDNN library in the process came from the drivers folder
(`live_linux_runtime_loads_from_drivers_dir`). Not yet re-run on a GPU. 3 is open.

## Files
`install.log`, `installed_files.txt` (what the installer unpacked), `machine.txt`, `status.txt`, `pulls.log` and `pull_*.log`, `server.log`, `server_highlights.log`, `checks.log`, `results.json`, `kokoro.wav`, `run.log`, `loaded_libs.txt` (empty: the server had already exited), `vram_margins.json`.
