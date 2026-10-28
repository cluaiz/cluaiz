# 🏛️ cluaiz: CI/CD & RELEASE ARCHITECTURE

This document is the entry point for the CI/CD pipeline. The full job list, file names and smoke test are in [docs/deployments/cicd.md](../../docs/deployments/cicd.md); hardware and OS coverage is in [docs/deployments/platforms.md](../../docs/deployments/platforms.md).

---

## 🧬 1. CORE + GPU DRIVERS

A release is one **core** per platform plus optional **GPU drivers**, the layout llama.cpp itself ships:

| Part | Contents | Built by |
|:---|:---|:---|
| **Core** | `cluaiz` CLI + API + engine, llama.cpp with every CPU variant (`GGML_CPU_ALL_VARIANTS`), ONNX Runtime for CPU (and DirectML on Windows). Links no GPU runtime, so it starts on any machine. | `release.yml` → `core` |
| **GGUF drivers** | One ggml backend library (`ggml-cuda` for CUDA 12 and 13, `ggml-vulkan`) from the same pinned fork commit, loaded by the core at start (`GGML_BACKEND_DL`), plus the redistributable CUDA runtime (`cudart`, `cuBLAS`). | `cluaize-llama-driver.yml` |
| **ONNX drivers** | ONNX Runtime's CUDA provider for CUDA 12 and 13, from Microsoft's release of the version the core links. cuDNN is downloaded by the engine on first use. | `cluaize-onnx-driver.yml` |

The display driver of the GPU itself is never shipped: it comes from NVIDIA, AMD or Intel.

---

## 🚀 2. ONE RELEASE PAGE: `release.yml`

- **Only by hand**: pushes and pull requests never start a build.
- **Run workflow** (manual, version `0.1.0`) builds the core for every platform, runs the smoke test (no GPU, fresh home, pull + answer), calls `cluaize-llama-driver.yml` and `cluaize-onnx-driver.yml` and publishes everything on one GitHub release `v<version>` with `SHA256SUMS.txt`. Each driver workflow can also be run alone to build its drivers without a release.
- **Partial success**: experimental platforms (Linux ARM64, macOS) and drivers may fail without blocking the release; a missing driver is simply absent from `SHA256SUMS.txt`, and the installers then keep that machine on the CPU.

The installers (`install.sh`, `install.ps1`) read `SHA256SUMS.txt` from the release, download only the files for the machine, and refuse any file whose checksum does not match.

The older split workflows (`cluaize-cmd`, `-engine`, `-kernel-*`, `publish-registry`) and their registry scripts were removed: they built package names that no longer exist and never produced a release.

---

## 📝 3. RELEASE NOTES

The release page text comes from `.github/release-notes.md` (install commands and which file each machine needs). Release Drafter was removed: it ran on every push but builds its changelog only from pull requests, which this repository does not use.

---

## 🏛️ 4. THE FOUNDER'S MANDATE

1. **Never Drift**: Do not change naming conventions or matrix structures once established. The installers rely on the file names (e.g., `cluaiz-0.1.0-linux-x64-llama-cuda12.tar.gz`).
2. **Standard over Ad-hoc**: Every fix must be architectural. Do not create inline quick-fixes.
3. **Decoupled by Design**: Each driver builds in its own job; a failing driver never blocks the core, and the release job only collects what built.

**This is the cluaiz Standard. Professional and Optimized.**
