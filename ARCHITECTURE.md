# 🏗️ cluaiz System Architecture

This document provides a deep technical breakdown of the **3-Tier Silicon-First** architecture that powers cluaiz.

## 🛰️ 1. The Design Philosophy: "Zero-Overhead Sovereignty"
cluaiz is designed to eliminate the "Abstraction Bloat" found in traditional inference engines. We speak directly to the silicon, bypassing Docker, VMs, and high-level Python wrappers.

---

## 🧬 2. The 3-Tier Hierarchy

### Tier I: cluaiz Runtime Engine
- **Role**: The high-performance orchestration hub.
- **Logic**: Manages the life-cycle of inference requests, FFI bindings, and memory synchronization.
- **Engine C**: The native ternary compute kernel resides here, optimized for AVX/Metal/CUDA.

### Tier II: The Brain (Relational Persistence Layer)
- **Role**: Infinite memory and state management.
- **Logic**: Utilizes **LanceDB** for vector embeddings and **SurrealDB** for relational state graphs.
- **State Stitching**: JIT mounting of historical context into the active inference window.

### Tier III: The Driver-Manager (JIT Hardware Provisioner)
- **Role**: Hardware-aware kernel management.
- **Logic**: Performs deep hardware probing (Silicon ID, VRAM, Compute Units) and pulls the exact versioned kernel from the registry.
- **DHL (Direct Hardware Linkage)**: Ensures the FFI handshake is zero-copy and atomically synchronized with host silicon.

---

## 🧵 3. The LogitSteer Protocol
LogitSteer is our proprietary method for **Physical State Injection**. 
- Instead of using prompt context, we inject behavioral rules directly into the **16-token kvcache.bin buckets**.
- This ensures 100% adherence to instructions without context drift or "Hallucination of Identity."

---

## 🛰️ 4. Silicon-Direct Workflow
1.  **Probe**: Detect CPU/GPU/NPU capabilities.
2.  **Bind**: JIT link the optimized shared library (.so/.dll).
3.  **Execute**: Pure ternary arithmetic (no matrix multiplication overhead).
4.  **Persist**: Store neural states in the Relational Brain.

---
**"Built on Rust. Born on Silicon."**
