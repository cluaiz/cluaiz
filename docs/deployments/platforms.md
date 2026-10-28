# Platform Support

Cluaiz ships as a **core** plus optional **GPU packs** (see [Release Workflow](cicd.md)). The core runs every model on the CPU and picks the fastest CPU code for the processor at start. A GPU pack in the same folder adds the GPU; when its driver or GPU is missing, ggml skips it and models stay on the CPU.

---

## Operating systems

| Platform | Core | Status |
|---|---|---|
| Windows 10 / 11 x64 | `win-x64` | Tested |
| Linux x64, glibc 2.38+ (Ubuntu 24.04, Debian 13, Fedora 39 or newer) | `linux-x64` | Tested on CPU (Ubuntu 24.04); GPU run pending |
| Linux ARM64, glibc 2.38+ | `linux-arm64` | Built in CI; not yet run on hardware |
| macOS Apple Silicon | `mac-arm64` | Built in CI; not yet run on hardware |

Older Linux (Ubuntu 22.04, Debian 12) is not supported yet: the bundled ONNX Runtime needs glibc 2.38.

## CPU

The core carries llama.cpp's CPU backend in several builds (x86: SSE4.2, AVX, AVX2, AVX-512, AVX-VNNI generations; ARM: NEON and newer). At start ggml loads the newest one the processor supports, so one download runs at full speed on old and new processors.

## GPUs

| GPU | GGUF models (llama.cpp) | ONNX models (speech, transcription) |
|---|---|---|
| NVIDIA, driver 580+ and RTX 20 series or newer | `llama-cuda13` + `cudart13` | `onnx-cuda13`; cuDNN is fetched from NVIDIA on first use |
| NVIDIA, older driver or GTX 900 / 10 series | `llama-cuda12` + `cudart12` | `onnx-cuda12`; cuDNN is fetched from NVIDIA on first use |
| AMD Radeon, Intel Arc (Windows, Linux) | `llama-vulkan` pack | CPU on Linux; DirectML on Windows (inside the core) |
| Apple Silicon | Metal, inside the macOS core | CPU |
| Integrated GPUs | CPU by default; the Vulkan pack can be installed by hand (`CLUAIZ_GPU=vulkan`) | as above |

Cluaiz never installs or ships the GPU driver itself. The driver comes from NVIDIA, AMD or Intel; the packs contain only llama.cpp's backend library and, for NVIDIA, the redistributable CUDA runtime (`cudart`, `cuBLAS`).

The GPU packs are built from the same llama.cpp fork commit as the core (`engine/llama/llama.cpp`), so a pack only works with the core of the same release.
