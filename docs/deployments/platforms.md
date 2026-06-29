# Platform Support

cluaiz operates as a universal neural kernel, enforcing compile-time validation for a diverse matrix of processors, backends, and operating systems.

---

## Supported OS Targets

The baseline CPU kernels are cross-compiled and validated for 9 primary platform profiles:

*   **Windows:** x86_64 (Desktop, Server, Surface platforms).
*   **Linux:**
    *   x86_64: Mainstream server architectures.
    *   Aarch64: Enterprise cloud edge configurations.
    *   ARMv7: Embedded IoT modules.
*   **macOS:**
    *   ARM64: Native Apple Silicon (M-series SOCs).
    *   x86_64: Intel-based Mac processors.
*   **Mobile OS:**
    *   Android (Aarch64): High-performance mobile devices.
    *   iOS (ARM64): iPhones and iPads.

---

## Execution Backends

The engine maps computing tasks across two distinct performance layers:

### 1. Baseline CPU Kernels (`cluaiz-kernel`)
Highly optimized CPU interpreters utilizing native SIMD instruction sets:
*   **Intel/AMD:** AVX512 and AVX2 vector extensions.
*   **Apple/ARM:** Native Neon vector engines.

### 2. Accelerator Drivers (`cluaiz-driver`)
Direct FFI interfaces bind specialized backends when accelerators are detected:
*   **NVIDIA:** CUDA v11, v12, and v13 runtimes.
*   **Apple:** Native Metal framework configurations.
*   **Universal:** Vulkan-based hardware pipelines.
*   **AMD:** ROCm and HIP compute backends.
*   **Intel:** OpenVINO inference runtimes.
