# 🏛️ CLUAIZE: CI/CD & RELEASE ARCHITECTURE

This document serves as the **Single Source of Truth** for the Cluaize Neural Ecosystem's CI/CD pipeline and release architecture. It defines the automated workflows, security protocols, and deployment laws that govern this repository.

> **AI Agent & Developer Notice:** Before modifying any workflow or release script, read this document thoroughly. The architecture is heavily decoupled and relies on a specific sequence to achieve Zero-Trust Security and seamless cross-platform deployments.

---

## 🧬 1. MODULAR ARCHITECTURE (THE NEURAL STACK)

The ecosystem is divided into four sovereign, decoupled layers. They are built separately but distributed together.

| Layer | Component | Responsibility |
| :--- | :--- | :--- |
| **Edge** | `cluaize-cli` | User interface, model management, and system orchestration. |
| **Brain** | `cluaize-engine` | The core orchestrator. Manages memory, context, and driver dispatch. |
| **Kernel** | `cluaize-kernel` | Base CPU/SIMD interpreters (AVX512, AVX2, NEON) compiled for 9 operating system targets. |
| **Bridge** | `cluaize-driver` | Specialized hardware/GPU drivers (CUDA, Metal, Vulkan, OpenVINO, ROCm, HIP). |

---

## 🚀 2. UNIFIED RELEASE PIPELINE (HOW IT WORKS)

We have abandoned fragmented, per-component releases (e.g., `cli-v1`, `kernel-v1`) in favor of a **Unified, Professional Release Architecture**. The pipeline operates in two distinct phases to ensure maximum stability and security.

### Phase 1: Parallel Matrix Compilation
When a new tag (e.g., `v1.2.0`) is pushed, 6 separate workflows trigger simultaneously:
1. `cluaize-cmd.yml` (Builds Windows/Linux/macOS CLI bundles)
2. `cluaize-engine.yml` (Builds the core orchestrators)
3. `cluaize-kernel-llama.yml` (Builds CPU instructions for Llama: AVX512, AVX2, NEON)
4. `cluaize-kernel-onnx.yml` (Builds CPU instructions for ONNX: AVX512, AVX2, NEON)
5. `cluaize-llama-driver.yml` (Builds GPU accelerators for Llama: CUDA, Metal, etc.)
6. `cluaize-onnx-driver.yml` (Builds GPU accelerators for ONNX: CUDA, CoreML, etc.)

**The Unified Tag Rule:** 
All workflows push their compiled binaries (`.exe`, `.so`, `.dylib`, `.zip`) directly to the **SAME** GitHub Release tag (`v1.2.0`). 

**Fault Tolerance (Partial Success):** 
The build matrices are designed with `fail-fast: false`. If an experimental driver (e.g., `linux-x64-cann`) fails to compile due to network timeouts or SDK issues, the workflow will **NOT** crash the entire release. The successful binaries will still be uploaded, and the failed binary will simply be omitted.

### Phase 2: Master Orchestration & Security (`publish-registry.yml`)
Because the 6 workflows run in parallel and finish at different times, they cannot generate the final registry themselves. 

Once all parallel builds finish, the **Publish Secure Registry** workflow is triggered manually via GitHub Actions (`workflow_dispatch`).
1. It downloads all raw binaries that were just uploaded to the GitHub Release.
2. It executes `.github/scripts/secure_registry_builder.py`.
3. It uploads the resulting `cluaize-registry.json` back to the GitHub Release.

---

## 🔒 3. ZERO-TRUST SECURITY REGISTRY

The core of the dynamic installation system is `secure_registry_builder.py`. 

Instead of hardcoding URLs or blindly trusting the build pipeline, this Python script performs cryptographic verification:
- It scans the downloaded release artifacts.
- It calculates a **SHA-256 checksum** for every single `.exe`, `.so`, `.dll`, and `.zip`.
- It dynamically generates `cluaize-registry.json`, categorizing assets into `components`, `kernels`, and `drivers`.
- **Self-Healing:** If a specific driver failed to compile in Phase 1, its binary won't exist. The Python script dynamically detects this and safely omits it from the JSON. The `cluaize-cli` will therefore never attempt to download a broken or non-existent driver.

---

## 📝 4. AUTOMATED CHANGELOGS (RELEASE DRAFTER)

To maintain professional, OpenClaw-style Release Notes without manual overhead, we utilize **Release Drafter** (`.github/workflows/release-drafter.yml`).

- As developers merge Pull Requests into `main`, the Drafter quietly categorizes them based on labels (`feature`, `bug`, `performance`, `documentation`).
- It continuously updates a **Draft Release** on the GitHub Releases page.
- When the team is ready to launch, the beautiful, formatted changelog (with `🚀 Highlights`, `⚡ Changes & Optimizations`, `🐛 Fixes`) is already written and ready to be published.

---

## 🏛️ 5. THE FOUNDER'S MANDATE
1. **Never Drift**: Do not change naming conventions or matrix structures once established. The Python registry builder relies on strict parsing of filenames (e.g., `cluaize-driver-v1.0.0-linux-x64-cuda-12.so`).
2. **Standard over Ad-hoc**: Every fix must be architectural. Do not create inline quick-fixes.
3. **Decoupled by Design**: Workflows must never wait on each other. Phase 1 is purely for compilation; Phase 2 is purely for aggregation and security.

**This is the Cluaize Standard. Professional. Optimized. Sovereign.**
