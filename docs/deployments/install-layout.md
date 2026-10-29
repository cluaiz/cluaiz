# What gets installed where

This page lists, file by file, what the installer puts on each operating system and which folders the engine
creates when it runs. The file lists and sizes are read from the v0.1.0 release archives; the folder layout is
the one after v0.1.1 (everything in `bin/` and `config/`).

## Summary

| OS | `~/.cluaiz/bin` after a CPU-only install | GPU files | Files in `bin` |
|---|---|---|---|
| **macOS** (Apple Silicon) | `cluaiz` only. llama.cpp, Metal and ONNX Runtime are inside it. | none (Metal is inside `cluaiz`) | 3 |
| **Windows** x64 | `cluaiz.exe` + llama.cpp DLLs + CPU variants + `DirectML.dll` | a GPU pack adds its DLLs to `bin` | 18, plus 1 to 6 with a GPU pack |
| **Linux** x64 | `cluaiz` + llama.cpp `.so` files + CPU variants | a GPU pack adds its `.so` files to `bin` | 32, plus 1 to 6 with a GPU pack |
| **Linux** ARM64 | `cluaiz` + llama.cpp `.so` files + ARM CPU variants | none | 26 |

ONNX Runtime (speech, transcription, embeddings, vision encoders) is inside the `cluaiz` binary on every OS. Only
its NVIDIA CUDA part comes as a separate pack.

On Windows and Linux, llama.cpp is in separate files because its GPU backends (`ggml-cuda`, `ggml-vulkan`) are
separate files that the installer adds only on a machine with that GPU. A backend loaded from its own file has
to share `ggml-base` with the program, so `ggml-base` and everything built on it (`ggml`, `llama`, `mtmd`) are
separate files too. The `ggml-cpu-*` files are one CPU backend per processor generation; the engine loads the
fastest one the processor supports. macOS has no separate GPU packs (Metal is part of the OS), so there
everything is one file.

## macOS (Apple Silicon)

```
~/.cluaiz/
└── bin/
    ├── cluaiz                      77.6 MB  CLI + API server + engine + llama.cpp + Metal + ONNX Runtime (CoreML)
    ├── LICENSE
    └── THIRD_PARTY_NOTICES.md
```

The installer picks no GPU pack on a Mac.

## Windows x64

### Core: `cluaiz-0.1.0-win-x64.zip` (every machine)

```
%USERPROFILE%\.cluaiz\
└── bin\
    ├── cluaiz.exe                  82.0 MB  CLI + API server + engine + ONNX Runtime (CPU)
    ├── DirectML.dll                18.5 MB  ONNX Runtime on any DirectX 12 GPU (AMD, Intel, NVIDIA)
    ├── llama.dll                    2.8 MB  llama.cpp
    ├── llama-common.dll             3.7 MB  llama.cpp chat templates and tool-call parsing
    ├── mtmd.dll                     1.4 MB  llama.cpp images and audio (multimodal)
    ├── ggml.dll                     0.1 MB  ggml backend loader
    ├── ggml-base.dll                0.7 MB  ggml tensors, shared with every backend
    ├── ggml-cpu-x64.dll             0.8 MB  ┐
    ├── ggml-cpu-sse42.dll           0.8 MB  │
    ├── ggml-cpu-sandybridge.dll     1.1 MB  │ CPU backend, one per processor generation;
    ├── ggml-cpu-haswell.dll         1.1 MB  │ the engine loads the best one at start
    ├── ggml-cpu-skylakex.dll        1.2 MB  │ (AVX, AVX2, AVX-512, ...)
    ├── ggml-cpu-cannonlake.dll      1.2 MB  │
    ├── ggml-cpu-cascadelake.dll     1.2 MB  │
    ├── ggml-cpu-icelake.dll         1.2 MB  │
    ├── ggml-cpu-alderlake.dll       1.1 MB  ┘
    ├── LICENSE
    └── THIRD_PARTY_NOTICES.md
```

### GPU packs (added to the same `bin\` by the installer)

| GPU | Packs | Files added to `bin\` |
|---|---|---|
| NVIDIA, driver for CUDA 13 (580+), RTX 30 / 40 / 50 | `llama-cuda13` | `ggml-cuda.dll` (148 MB) |
| | `cudart13` | `cudart64_13.dll`, `cublas64_13.dll` (55 MB), `cublasLt64_13.dll` (493 MB) |
| | `onnx-cuda13` | `onnxruntime_providers_cuda.dll` (268 MB), `onnxruntime_providers_shared.dll` |
| Other NVIDIA (CUDA 12) | `llama-cuda12` | `ggml-cuda.dll` (570 MB) |
| | `cudart12` | `cudart64_12.dll`, `cublas64_12.dll` (100 MB), `cublasLt64_12.dll` (474 MB) |
| | `onnx-cuda12` | `onnxruntime_providers_cuda.dll` (497 MB), `onnxruntime_providers_shared.dll` |
| AMD, Intel Arc | `llama-vulkan` | `ggml-vulkan.dll` (45 MB) |
| no GPU | none | none |

### Full tree on a Windows PC with an NVIDIA GPU (CUDA 13)

Every file, with where it came from. GPU drivers are marked; the rest is the core.

```
%USERPROFILE%\.cluaiz\
├── bin\
│   ├── cluaiz.exe                          core         CLI + API + engine + ONNX Runtime (CPU)
│   ├── DirectML.dll                        core         ONNX GPU driver for any DirectX 12 GPU
│   ├── llama.dll, llama-common.dll, mtmd.dll   core     llama.cpp
│   ├── ggml.dll, ggml-base.dll             core         ggml
│   ├── ggml-cpu-*.dll  (9 files)           core         GGUF CPU drivers
│   ├── ggml-cuda.dll                       llama-cuda13 GGUF GPU driver (NVIDIA)          ◄ GPU
│   ├── cudart64_13.dll                     cudart13     CUDA runtime for ggml-cuda        ◄ GPU
│   ├── cublas64_13.dll, cublasLt64_13.dll  cudart13     CUDA math for ggml-cuda           ◄ GPU
│   ├── onnxruntime_providers_cuda.dll      onnx-cuda13  ONNX GPU driver (NVIDIA)          ◄ GPU
│   ├── onnxruntime_providers_shared.dll    onnx-cuda13  link between ONNX Runtime and it  ◄ GPU
│   ├── cudnn64_9.dll + 7 cudnn_*64_9.dll   download     cuDNN 9 for the ONNX GPU driver   ◄ GPU
│   ├── cufft64_12.dll                      download     cuFFT for the ONNX GPU driver     ◄ GPU
│   ├── cuda_runtime.ready                  download     which CUDA major these are for
│   ├── LICENSE
│   └── THIRD_PARTY_NOTICES.md
├── config\                                 settings, permissions, registries, hardware scan
└── models\
```

"download" files are not in any pack: the engine downloads them from NVIDIA (about 1 GB) the first time an
ONNX model (speech, transcription) runs on the GPU. On an AMD or Intel Arc PC, `bin\` has the core plus
`ggml-vulkan.dll` (GGUF GPU driver), and ONNX models use `DirectML.dll` from the core. On a PC without a GPU,
`bin\` has the core only.

## Linux x64

### Core: `cluaiz-0.1.0-linux-x64.tar.gz` (every machine)

```
~/.cluaiz/
└── bin/
    ├── cluaiz                      81.0 MB  CLI + API server + engine + ONNX Runtime (CPU)
    ├── libllama.so.0.6.0            4.9 MB  llama.cpp      (+ libllama.so, libllama.so.0 links)
    ├── libllama-common.so.0.6.0     6.3 MB  chat templates (+ 2 links)
    ├── libmtmd.so.0.6.0             1.9 MB  multimodal     (+ 2 links)
    ├── libggml.so.0.26.0            0.1 MB  backend loader (+ 2 links)
    ├── libggml-base.so.0.26.0       1.0 MB  ggml tensors   (+ 2 links)
    ├── libggml-cpu-x64.so           ┐
    ├── libggml-cpu-sse42.so         │
    ├── libggml-cpu-sandybridge.so   │
    ├── libggml-cpu-ivybridge.so     │
    ├── libggml-cpu-piledriver.so    │ CPU backend, one per processor generation
    ├── libggml-cpu-haswell.so       │ (1.0 to 1.9 MB each)
    ├── libggml-cpu-skylakex.so      │
    ├── libggml-cpu-cannonlake.so    │
    ├── libggml-cpu-cascadelake.so   │
    ├── libggml-cpu-cooperlake.so    │
    ├── libggml-cpu-icelake.so       │
    ├── libggml-cpu-alderlake.so     │
    ├── libggml-cpu-sapphirerapids.so│
    ├── libggml-cpu-zen4.so          ┘
    ├── LICENSE
    └── THIRD_PARTY_NOTICES.md
```

### GPU packs (added to the same `bin/` by the installer)

| GPU | Packs | Files added to `bin/` |
|---|---|---|
| NVIDIA, driver for CUDA 13 | `llama-cuda13` | `libggml-cuda.so` (163 MB) |
| | `cudart13` | `libcudart.so.13`, `libcublas.so.13` (58 MB), `libcublasLt.so.13` (545 MB) |
| | `onnx-cuda13` | `libonnxruntime_providers_cuda.so` (280 MB), `libonnxruntime_providers_shared.so` |
| Other NVIDIA (CUDA 12) | `llama-cuda12` | `libggml-cuda.so` (177 MB) |
| | `cudart12` | `libcudart.so.12`, `libcublas.so.12` (116 MB), `libcublasLt.so.12` (752 MB) |
| | `onnx-cuda12` | `libonnxruntime_providers_cuda.so` (621 MB), `libonnxruntime_providers_shared.so` |
| AMD, Intel Arc | `llama-vulkan` | `libggml-vulkan.so` (35 MB) |

## Linux ARM64

```
~/.cluaiz/
└── bin/
    ├── cluaiz                      76.2 MB
    ├── libllama, libllama-common, libmtmd, libggml, libggml-base (.so, as on x64)
    ├── libggml-cpu-armv8.0_1.so    ┐
    ├── libggml-cpu-armv8.2_1.so    │
    ├── libggml-cpu-armv8.2_2.so    │
    ├── libggml-cpu-armv8.2_3.so    │ CPU backend per ARM generation (NEON, dot product, SVE, ...)
    ├── libggml-cpu-armv8.6_1.so    │
    ├── libggml-cpu-armv8.6_2.so    │
    ├── libggml-cpu-armv9.2_1.so    │
    ├── libggml-cpu-armv9.2_2.so    ┘
    ├── LICENSE
    └── THIRD_PARTY_NOTICES.md
```

No GPU packs for ARM64.

## Folders the engine creates when it runs (every OS)

```
~/.cluaiz/
├── bin/                         the installed files above, plus on NVIDIA the libraries the engine
│                                downloads for ONNX on the GPU (about 1 GB, on first use):
│                                  cuDNN 9   Windows: cudnn64_9.dll + 7 cudnn_*64_9.dll
│                                            Linux:   libcudnn.so.9 + 7 libcudnn_*.so.9
│                                  cuFFT, and cuRAND on Linux (+ cudart / cuBLAS when not already there)
│                                  cuda_runtime.ready: which CUDA major these files are for
├── config/
│   ├── settings.json            model and generation settings (cluaiz config)
│   ├── permission.json          host, port, auth, execution policy (cluaiz permission)
│   ├── hardware_profile.json    cached hardware scan (cluaiz calibrate)
│   ├── vram_margins.json        VRAM each model really took, for the next plan
│   ├── model_registry.json
│   └── tools_registry.json
├── session.token                API token
├── models/
│   ├── chat/                    GGUF chat and vision models
│   ├── embedding/
│   ├── tts/                     text-to-speech
│   ├── stt/                     speech-to-text
│   └── ingest/                  document and vision encoders
├── tools/
│   ├── skills/
│   ├── plugins/
│   └── mcp/
├── kv_cache/
└── reports/
```

Up to v0.1.1 there was also an `engine/` folder: `engine/config/` (now `config/`), `engine/drivers/` (now in
`bin/`), `engine/media/` (now `bin/media/`) and `engine/vram_margins.json` (now in `config/`). A newer `cluaiz`
moves them at its first start and removes `engine/`.

`CLUAIZ_HOME` moves this whole tree (used by tests). The installer's `CLUAIZ_INSTALL_DIR` moves only `bin`.

## Who loads what, and from where

| File | Loaded by | Looked for in |
|---|---|---|
| `llama`, `ggml`, `ggml-base`, `mtmd`, `llama-common` | the operating system, when `cluaiz` starts | the folder of `cluaiz` |
| `ggml-cpu-*`, `ggml-cuda`, `ggml-vulkan` | llama.cpp (`ggml_backend_load_all_from_path`) | the folder of `cluaiz` |
| `cudart`, `cublas`, `cublasLt` | the operating system, when `ggml-cuda` loads | the folder of `cluaiz` |
| `onnxruntime_providers_cuda`, `_shared` | ONNX Runtime | only the folder of `cluaiz` (never `PATH`) |
| cuDNN, cuRAND, cuFFT | ONNX Runtime's CUDA provider | the folder of `cluaiz` (`bin`); on Linux the engine loads them by full path first |
| `DirectML.dll` | ONNX Runtime (Windows) | the folder of `cluaiz` |
