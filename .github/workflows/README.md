# 🏛️ CLUAIZE-OS: THE SOVEREIGN SYSTEM DESIGN

This document serves as the **Single Source of Truth** for the Cluaize Neural Ecosystem. It defines the architectural DNA, industrial standards, and universal deployment laws that govern every line of code in this repository.

---

## 🛰️ 1. THE VISION: UNIVERSAL NEURAL SOVEREIGNTY
Cluaize-OS is designed to be a **Universal Neural Kernel**. Our mission is to provide high-performance, native inference for any model, on any silicon, under any operating system, eliminating hardware boundaries.

### 🚀 Core Directives:
- **Silicon Mastery**: Extract peak performance from CPU, GPU, NPU, and TPU natively.
- **Hardware Agnosticism**: Unified execution across Windows, Linux, Android, iOS, and macOS.
- **Modular Integrity**: Decoupled architecture where components communicate via standardized FFI handshakes.

---

## 🧬 2. MODULAR ARCHITECTURE (THE NEURAL STACK)

The ecosystem is divided into four sovereign, decoupled layers. Any change to one layer MUST NOT break the handshake of others.

| Layer | Component | Responsibility |
| :--- | :--- | :--- |
| **Edge** | `cluaize-cli` | User interface, model management, and system orchestration. |
| **Brain** | `cluaize-engine` | The core orchestrator. Manages memory, context, and driver dispatch. |
| **Kernel** | `cluaize-kernel` | Base CPU/SIMD interpreters (AVX512, AVX2, NEON) compiled for 9 operating system targets. |
| **Bridge** | `cluaize-driver` | Specialized hardware/GPU drivers (CUDA, Metal, Vulkan, OpenVINO, ROCm, HIP). |

---

## 🛰️ 3. THE UNIVERSAL MATRIX LAW
Cluaize-OS MUST run everywhere. The baseline CPU kernels and drivers support:

- **Windows**: x64 (Desktop/Surface/Server).
- **Linux**: x86_64 (Server), aarch64 (Cloud/Edge), armv7 (IoT).
- **Android**: aarch64 (Mobile/Tablet/Auto).
- **macOS**: arm64 (Apple Silicon), x86_64 (Intel Mac).
- **iOS**: arm64 (iPhone/iPad).

---

## 💎 4. THE NAMING & VERSIONING CONSTITUTION
To ensure zero-latency binary mapping, all artifacts MUST follow the **Sovereign Naming Convention**:

### 📦 Baseline CPU Kernels:
`cluaize-kernel-<version>-<platform>.<ext>`
- **Platform**: Matches the 9 core OS targets (e.g., `win-x64-avx512`, `linux-x64-avx2`, `linux-arm64`, `mac-arm64`, etc.).
- **Releases**: Pushed to `kernel-v*` release tags.

### 🔌 Specialized Accelerator Drivers:
`cluaize-driver-<version>-<platform>-<backend>.<ext>`
- **Backend**: Specialized silicon modules (e.g., `cuda-v13`, `cuda-v12`, `cuda-v11`, `metal`, `vulkan`, `openvino`, `rocm`, `hip`).
- **Releases**: Pushed to `driver-v*` release tags and indexed in `registry.json`.

---

## ⚡ 5. CI/CD PIPELINE INTEGRITY (ZERO-CRASH DEPLOYMENT)
The GitHub Actions pipelines are divided into **5 distinct, highly-decoupled factories**:

### ⚙️ 1. `cluaize-cmd.yml` (The Edge CLI)
- **Compilation**: Parallel builds for 6 OS/Architecture combinations (Windows, Linux, macOS for both x64 and arm64).
- **Releases**: Uploads the main entrypoint executables to `cli-v*` release tags.

### ⚙️ 2. `cluaize-kernel-llama.yml` & `cluaize-kernel-onnx.yml` (Silicon Kernels)
- **Compilation**: Parallel builds for exactly 9 core platforms using CPU instructions (AVX512, AVX2, NEON).
- **Tooling**: Uses `cross` for Docker-based cross-compilation on target architectures (Android, Linux Aarch64).
- **Releases**: Uploads baseline library binaries to `kernel-v*` and `onnx-kernel-v*` release tags.

### ⚙️ 3. `cluaize-llama-driver.yml` & `cluaize-onnx-driver.yml` (Dynamic Accelerators)
- **Compilation**: Parallel builds for specialized backend matrices (CUDA v13/v12/v11, Metal, Vulkan, OpenVINO, ROCm, HIP, SYCL, CANN, QNN).
- **Packaging Strategy**: 
  - **Llama Drivers**: Uploaded as direct `.dll` / `.so` / `.dylib` binaries.
  - **ONNX Drivers**: Bundled as **Modular `.zip` Files** containing `cluaize_onnx.dll` AND all necessary provider libraries (e.g., `onnxruntime_providers_cuda.dll`). This guarantees that downloading one asset provides all underlying SDK dependencies for the target hardware.
- **Releases**: Uploads accelerator driver binaries/zips to `driver-v*` and `onnx-driver-v*` release tags.

### 📁 4. The Flat Engine Law (1:1 Deploy Parity)
- **The Rule**: Production MUST exactly mirror Local Dev (`.cluaize/engine/`).
- **No Sub-folders**: Kernels and Core binaries live directly in `engine/`. Hardware drivers (and their ONNX providers) live directly in `engine/drivers/`. Legacy `interfaces/kernels` and `interfaces/drivers` sub-directories are strictly banned to ensure the OS Loader can resolve dependencies natively.

### 🧠 4. Dynamic Manifest Registry (Python Automation)
- **The Old Flaw**: Manifests were hardcoded via `cat <<EOF`. If a build (like SYCL) failed, the JSON would still include the broken link, crashing the Engine on startup.
- **The New Standard**: Every workflow features a dynamic Python script during the `publish-registry` job. It aggressively scans the `artifacts/` folder and generates a 100% accurate `cluaize-*.json` manifest containing **ONLY successful binaries**. Failed matrix targets are safely and automatically omitted.

---

## 🏛️ 6. THE FOUNDER'S MANDATE
1. **Never Drift**: Do not change naming conventions or matrix structures once established.
2. **Standard over Ad-hoc**: Every fix must be architectural, not a "Kach-Khas" (quick-fix).
3. **Total Coverage**: A build is only successful if ALL platforms in the matrix pass.

**This is the Cluaize Standard. Professional. Optimized. Sovereign.**
