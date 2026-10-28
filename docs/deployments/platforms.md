# Platform Support

Cluaiz ships as a **core** plus optional **GPU packs** (see [Release Workflow](cicd.md)). The core runs every model on the CPU and picks the fastest CPU code for the processor at start. A GPU pack in the same folder adds the GPU; when its driver or GPU is missing, ggml skips it and models stay on the CPU.

---

## Operating systems

| Platform | Core | Status |
|---|---|---|
| Windows 10 / 11 x64 | `win-x64` | Tested |
| Linux x64, glibc 2.38+ (Ubuntu 24.04, Debian 13, Fedora 39 or newer) | `linux-x64` | Tested (Ubuntu 24.04: CPU, and NVIDIA Tesla T4 with the CUDA 12 pack) |
| Linux ARM64, glibc 2.38+ | `linux-arm64` | Built in CI; not yet run on hardware |
| macOS Apple Silicon | `mac-arm64` | Built in CI; not yet run on hardware |

Older Linux (Ubuntu 22.04, Debian 12) is not supported yet: the bundled ONNX Runtime needs glibc 2.38.

## CPU

The core carries llama.cpp's CPU backend in several builds (x86: SSE4.2, AVX, AVX2, AVX-512, AVX-VNNI generations; ARM: NEON and newer). At start ggml loads the newest one the processor supports, so one download runs at full speed on old and new processors.

## GPUs

| GPU | GGUF models (llama.cpp) | ONNX models (speech, transcription) |
|---|---|---|
| NVIDIA RTX 30 / 40 / 50, driver with CUDA 13 (580+) | `llama-cuda13` + `cudart13` | `onnx-cuda13`; cuDNN is fetched from NVIDIA on first use |
| NVIDIA RTX 20, T4 and other Turing, driver with CUDA 13.4 or newer | `llama-cuda13` + `cudart13` | `onnx-cuda13` |
| NVIDIA with an older driver, or GTX 900 / 10 series, Volta | `llama-cuda12` + `cudart12` | `onnx-cuda12`; cuDNN is fetched from NVIDIA on first use |
| NVIDIA with a driver older than CUDA 12.8 (Linux) / 12.4 (Windows), except RTX 30 / 40 | `llama-vulkan` (update the driver for CUDA) | CPU |
| AMD Radeon, Intel Arc (Windows, Linux) | `llama-vulkan` pack | CPU on Linux; DirectML on Windows (inside the core) |
| Apple Silicon | Metal, inside the macOS core | CPU |
| Integrated GPUs | CPU by default; the Vulkan pack can be installed by hand (`CLUAIZ_GPU=vulkan`) | as above |

The CUDA packs carry ready-made GPU code only for RTX 30, 40 and 50 (llama.cpp's default build); for other NVIDIA GPUs the driver compiles the code on first use, which needs a driver at least as new as the pack's CUDA toolkit (CUDA 13.4 and 12.8 on Linux, 13.4 and 12.4 on Windows). `nvidia-smi` prints the newest CUDA version the driver supports, and the installers choose the pack from it. Example: a Tesla T4 with driver 580 (CUDA 13.0) gets CUDA 12.

Cluaiz never installs or ships the GPU driver itself. The driver comes from NVIDIA, AMD or Intel; the packs contain only llama.cpp's backend library and, for NVIDIA, the redistributable CUDA runtime (`cudart`, `cuBLAS`).

The GPU packs are built from the same llama.cpp fork commit as the core (`engine/llama/llama.cpp`), so a pack only works with the core of the same release.
