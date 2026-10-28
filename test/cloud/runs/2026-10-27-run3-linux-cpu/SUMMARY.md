# Run 3 — Linux, CPU only (2026-10-27)

> **Summary.** The engine built and ran on Linux for the first time. The 366 unit tests of 10 crates pass,
> 7 models came down correctly with `cluaiz pull`, the 4B / 12B / 24B chat models answered correctly on the CPU,
> and the Kokoro → Whisper round trip read its own voice back ("Rivers carry water to the sea."). The Linux build
> hit 7 problems, all fixed (below). The 4 Whisper failures were a bug in the pull bundler (`95165aa`), not in
> Linux.

## Machine
| | |
|---|---|
| VM | Google Cloud `n2-standard-8`, Spot, us-central1-a |
| CPU | Intel Xeon @ 2.80 GHz, 8 vCPU |
| RAM | 31 GiB |
| GPU | none |
| OS | Ubuntu 24.04 (Deep Learning VM image `common-cu129-ubuntu-2404-nvidia-580`, CUDA 12.9 toolkit) |
| Rust | stable (`rustc` from rustup) |
| Build | `cargo build -p cmd --release`, `CUDA_ARCHITECTURES=75;89`, 1157 s on 8 vCPU |

## Results
**Unit tests:** 366 passed, 0 failed (`unit_tests.log`).

| Crate | Passed |
|---|---|
| core 36 · plan 24 · models 25 · media 42 · context 28 · agents 63 · cep 19 · runtime 18 · onnx 57 · api 54 | 366 |

**Pulls** (`pulls.log`): all 7 exit 0. Qwen3-4B 17 s, Gemma-3 12B 55 s, Mistral-Small-3.2 24B 192 s, bge-m3 6 s, Kokoro 17 s, Whisper turbo 28 s, Qwen3-TTS 13 s.

**Chat** (Auto settings; the planner chose CPU and the 4096 floor every time):

| Model (Q4_K_M) | Prompt | Placement | Context | Prompt t/s | Gen t/s | Time |
|---|---|---|---|---|---|---|
| Qwen3-4B | short | CpuOnly | 4096 | 44.0 | 10.5 | 23 s |
| Qwen3-4B | ~1.2k tokens | CpuOnly | 4096 | 49.7 | 5.8 | 50 s |
| Gemma-3 12B | short | CpuOnly | 4096 | 10.3 | 3.6 | 64 s |
| Gemma-3 12B | ~1.2k tokens | CpuOnly | 4096 | 18.8 | 2.8 | 121 s |
| Mistral-Small-3.2 24B | short | CpuOnly | 4096 | 10.0 | 1.9 | 168 s |
| Mistral-Small-3.2 24B | ~1.2k tokens | CpuOnly | 4096 | 9.8 | 1.8 | 237 s |

All replies were on topic (`checks.log`).

**Speech, transcription, embeddings, mixed rounds** (25 calls in `results.json`): 21 passed. The 4 failures were all Whisper: `Ambiguous uncached STT decoder graphs`. Cause: the pull bundled both the `int8` and the `uint8` decoders, because the quant tag list matched `INT8` inside `UINT8`. Fixed in `95165aa`; re-pulled on the same VM, the bundle has only the int8 graphs and the transcription of Kokoro's own speech is exact: `"Rivers carry water to the sea."` (9.3 s first call, 3.4 s loaded; `stt_recheck.log`).

| Call | Result |
|---|---|
| Kokoro (ONNX, uint8) | 5.2 s first, 2.7–3.8 s after |
| bge-m3 (GGUF) | 4.6 s first, 0.05–1.0 s after |
| Qwen3-TTS 1.7B (GGUF) | 16–25 s per sentence on CPU |
| Whisper turbo int8 (ONNX) | after the fix: 9.3 s first, 3.4 s loaded |

## Linux build problems found and fixed in this run
| # | Problem | Fix |
|---|---|---|
| 1 | A workspace member depended on `../../../cluaiz-tools`, so no clone of this repo (nor CI) could build | `test/comprehensive_e2e_trace` is its own workspace, excluded from the engine's |
| 2 | On Linux the CUDA backend (`ggml-cuda`) and the CUDA runtime were never linked | `engine/llama/build.rs`: link them on Linux (whole archive, before `ggml-base`) |
| 3 | `rfd` (folder dialog) needed Wayland at build time and `libwayland-client` at run time | `wayland-sys` with `dlopen`: loaded only when a dialog opens |
| 4 | `ort` asked for `cuda + directml`; DirectML is Windows only, so no Linux download exists | DirectML only on Windows; `lax-feature-matching` on Linux |
| 5 | llama.cpp's CPU code uses OpenMP (gcc); `libgomp` was not linked | link `gomp` on Linux |
| 6 | The prebuilt ONNX Runtime needs glibc ≥ 2.38 / GCC 13 libstdc++: Ubuntu 22.04 cannot link | built on Ubuntu 24.04 (open decision below) |
| 7 | Opus was linked to the system `libopus.so` | `audiopus_sys` `static` on Linux |
| 8 | Default `CUDA_ARCHITECTURES` is `86;89`: no kernels for T4 / RTX 20xx / GTX 16xx (sm_75) | this run used `75;89` (open decision below) |

Build dependencies on Ubuntu: `build-essential cmake pkg-config clang libclang-dev libssl-dev autoconf automake libtool` (`test/cloud/remote/linux_build.sh`).

## Open decisions (owner)
1. **Linux glibc floor.** The binary needs glibc 2.38+ (Ubuntu 24.04, Debian 13, Fedora 39+). Supporting Ubuntu 22.04 / Debian 12 means loading ONNX Runtime dynamically (`ort` `load-dynamic`, ship `libonnxruntime.so`).
2. **CUDA architectures in release builds.** Adding `75` covers T4, RTX 20xx and GTX 16xx; it makes the build longer and the binary larger.
3. **CUDA libraries are linked, not loaded.** The binary needs `libcudart` / `libcublas` / `libcuda` to start, so a Linux machine without an NVIDIA driver cannot run this build. A CPU-only machine needs a CPU build (or ggml's dynamic backend loading).
4. `Cargo.lock` is in `.gitignore`: CI resolves dependencies fresh on every build, so two builds of the same commit can differ.

## Cost
Run 3 VM (n2-standard-8 Spot, ~1.6 h): about $0.10. All of 2026-10-27 including the T4 attempts that Google reclaimed: about $0.5. The VM is stopped; its 100 GB disk (build + models) is kept for the GPU runs (~$10 / month while it exists).

## Files
`machine.txt`, `build.log` (full build), `build_relink.log` (static Opus), `unit_tests.log` (+ `_full`), `pulls.log` (+ one log per model), `checks.log`, `results.json`, `server_highlights.log`, `stt_recheck.log`. Not committed: `server.log`, `kokoro.wav`.
