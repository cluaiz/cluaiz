# 🖥️ Cluaiz Hardware (`engine/hardware`)

Single Source of Truth (SSOT) hardware prober and metric evaluator for Cluaiz.

## 📐 Architecture

```mermaid
graph TD
    A[HardwareProber] -->|sysinfo| B[CPU / RAM / Disks]
    A -->|NVML wrapper| C[NVIDIA VRAM / Cores / PCIe]
    A -->|Cache Layer| D[HardwareSnapshot in RAM]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`types.rs`](src/types.rs) | Typed data structures for CPU, RAM, NVMe/SSD, and GPU metrics. |
| [`prober.rs`](src/prober.rs) | Hardware discovery and fast (<5ms) live RAM/VRAM memory queries. |
