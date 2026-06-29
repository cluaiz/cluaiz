# 🛰️ cluaiz-OS: Independent Inference Drivers (SID)

## 🏛️ **Architecture Overview: The Hardware Abstraction Layer (HAL)**
The `inference-drivers` module is the **"Silicon Heart"** of cluaiz-OS. It serves as a unified Hardware Abstraction Layer (HAL) that bridges our high-level Neural Kernels (`Candle`, `LLaMA`, `BitNet`) with the raw physical silicon (NPU, GPU, TPU, CPU).

Our philosophy is **"Universal Sovereignty"**: The same cluaiz instance must be able to run on any OS and any hardware with zero performance penalty.

---

## 🧠 **The Driver-Kernel Handshake**

### **1. Universal Execution Providers**
Our Kernels are designed to be "Hardware-Agnostic" at the source level but "Hardware-Native" at the execution level. 
- **Candle Engine:** Natively interfaces with our `cuda`, `metal`, and `vulkan` drivers via its internal JIT/AOT math kernels.
- **LLaMA (llama.cpp) Engine:** Seamlessly switches between `OpenVINO`, `QNN`, `ROCm`, and `Metal` by dynamically linking the corresponding driver bridge from this directory.
- **BitNet / Mamba:** Specialized kernels that utilize our `cpu-blas` and `ghost` drivers for high-efficiency integer math.

### **2. Zero-Copy Memory Pointers**
The `driver-manager` utilizes `archer-shared` to create a **Zero-Copy Memory Map**. When a driver (e.g., `cuda/v12.1`) is loaded, it shares the exact same memory address space with the Inference Engine. This eliminates the "Data Copy Penalty" that plagues traditional AI frameworks.

---

## 📂 **The Independent Driver Matrix**

| Hardware Driver | Support Parity | Optimization Target |
| :--- | :--- | :--- |
| **CUDA** | Windows / Linux | NVIDIA RTX / Tesla (v11.8 & v12.1) |
| **Metal** | macOS / iOS / Darwin | Apple Silicon (M1/M2/M3/M4) |
| **OpenVINO** | Windows / Linux | Intel Core Ultra NPUs & Iris Xe GPUs |
| **QNN** | Windows / Android | Qualcomm Snapdragon X Elite / Mobile |
| **Vulkan** | Win / Linux / Android / macOS | Universal GPU Fallback (MoltenVK on Mac) |
| **ROCm** | Windows / Linux | AMD Radeon / Instinct Series |
| **CPU-BLAS** | ALL Operating Systems | Universal Legacy/Edge CPU Math |
| **Ghost** | Virtualized / Mock | Mock-Silicon for Testing & Privacy-First Execution |

---

## 🚦 **Industrial versioning (Why v11.8 vs v12.1?)**
Unlike other systems that assume the user has the latest setup, cluaiz-OS is designed for the real world.
- **Legacy Support:** Older NVIDIA GPUs (Pascal/Turing) often require `v11.8` for stability.
- **Modern Mastery:** Newer 40-series and 50-series GPUs utilize `v12.1+` for maximum compute throughput.
The `SovereignLinker` automatically detects the hardware generation and provisions the correct versioned binary from the cloud foundry.

---

## 🛠️ **Installation & Linking**
Drivers are not "installed" in the traditional sense; they are **"Provisioned"**.
1. `driver-manager` scans the hardware.
2. It fetches the signed binary from the [Independent Foundry Matrix](https://github.com/cluaiz/cluaiz/releases/tag/latest-kernels).
3. It validates the SHA-256 hash via the `kernel-manifest.json`.
4. It performs a **Dynamic Handshake** using the `archer_kernel_init` symbol to attach the driver to the engine's memory.

---

## 🗺️ **Independent Logic Flow: The Silicon-to-Kernel Lifecycle**

```mermaid
graph TD
    subgraph "Phase 1: Silicon Discovery"
        HW_SCAN["[driver-manager] Hardware Scan"]
        HW_SCAN -- "Identifies NVIDIA / Apple / Intel / ARM" --> PROFILE["Generate SovereignProfile"]
    end

    subgraph "Phase 2: Dynamic Provisioning"
        PROFILE -- "Query manifest.json" --> FOUNDRY["[Independent Foundry] GitHub Releases"]
        FOUNDRY -- "Download Native Binary (.dll/.so/.dylib)" --> LOCAL_STORAGE["Secure Kernel Cache"]
    end

    subgraph "Phase 3: The Independent Handshake"
        LOCAL_STORAGE -- "Zero-Copy Linkage" --> MEM_MAP["Shared Memory Map (archer-shared)"]
        MEM_MAP -- "Dynamic Handshake (FFI)" --> KERNEL["Interface Engine (Candle/LLaMA)"]
    end

    subgraph "Phase 4: Native Execution"
        KERNEL -- "Direct Silicon Access" --> GPU_NPU["Hardware Execution (Silicon Native)"]
    end

    %% Styling
    style HW_SCAN fill:#2d3436,stroke:#00cec9,stroke-width:2px,color:#fff
    style FOUNDRY fill:#2d3436,stroke:#0984e3,stroke-width:2px,color:#fff
    style MEM_MAP fill:#2d3436,stroke:#6c5ce7,stroke-width:2px,color:#fff
    style GPU_NPU fill:#2d3436,stroke:#d63031,stroke-width:2px,color:#fff
```

**"Silicon Agnostic. Native Performance. Universal Sovereignty."**
 