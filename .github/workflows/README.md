# 🏛️ cluaiz: CI/CD & RELEASE ARCHITECTURE

This document is the entry point for the CI/CD pipeline. The full job list, file names and smoke test are in [docs/deployments/cicd.md](../../docs/deployments/cicd.md); hardware and OS coverage is in [docs/deployments/platforms.md](../../docs/deployments/platforms.md).

---

## 🧬 1. CORE + GPU PACKS

A release is one **core** per platform plus optional **GPU packs**, the layout llama.cpp itself ships:

| Part | Contents | Built by |
|:---|:---|:---|
| **Core** | `cluaiz` CLI + API + engine, llama.cpp with every CPU variant (`GGML_CPU_ALL_VARIANTS`), ONNX Runtime for CPU (and DirectML on Windows). Links no GPU runtime, so it starts on any machine. | `release.yml` → `core` |
| **GGUF GPU pack** | One ggml backend library (`ggml-cuda`, `ggml-vulkan`) from the same pinned fork commit, loaded by the core at start (`GGML_BACKEND_DL`). | `release.yml` → `gpu-*` |
| **NVIDIA runtime pack** | The redistributable CUDA runtime the CUDA backend links (`cudart`, `cuBLAS`). | `release.yml` → `gpu-*` |
| **ONNX CUDA pack** | ONNX Runtime's CUDA provider. | `release.yml` → `core` |

The GPU driver is never shipped: it comes from NVIDIA, AMD or Intel.

---

## 🚀 2. ONE WORKFLOW: `release.yml`

- **Every push / pull request** touching engine, CLI or API code builds the core for every platform and runs the smoke test (no GPU, fresh home, pull + answer). A broken build is caught before a release.
- **Run workflow** (manual, version `0.1.0`) additionally builds the GPU packs and publishes the GitHub release `v<version>` with `SHA256SUMS.txt`.
- **Partial success**: experimental platforms (Linux ARM64, macOS) and GPU packs may fail without blocking the release; a missing pack is simply absent from `SHA256SUMS.txt`, and the installers then keep that machine on the CPU.

The installers (`install.sh`, `install.ps1`) read `SHA256SUMS.txt` from the release, download only the files for the machine, and refuse any file whose checksum does not match.

The older split workflows (`cluaize-*.yml`, `publish-registry.yml`) and their registry scripts were removed: they built package names that no longer exist and never produced a release.

---

## 📝 3. AUTOMATED CHANGELOGS (RELEASE DRAFTER)

To maintain professional, OpenClaw-style Release Notes without manual overhead, we utilize **Release Drafter** (`.github/workflows/release-drafter.yml`).

- As developers merge Pull Requests into `main`, the Drafter quietly categorizes them based on labels (`feature`, `bug`, `performance`, `documentation`).
- It continuously updates a **Draft Release** on the GitHub Releases page.
- When the team is ready to launch, the beautiful, formatted changelog (with `🚀 Highlights`, `⚡ Changes & Optimizations`, `🐛 Fixes`) is already written and ready to be published.

---

## 🏛️ 4. THE FOUNDER'S MANDATE

1. **Never Drift**: Do not change naming conventions or matrix structures once established. The installers rely on the file names (e.g., `cluaiz-0.1.0-linux-x64-llama-cuda12.tar.gz`).
2. **Standard over Ad-hoc**: Every fix must be architectural. Do not create inline quick-fixes.
3. **Decoupled by Design**: A failing GPU pack never blocks the core; the release job only collects what built.

**This is the cluaiz Standard. Professional and Optimized.**
