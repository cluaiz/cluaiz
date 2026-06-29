# Introduction to cluaiz

cluaiz is a high-performance local AI inference engine and orchestrator designed to execute Large Language Models (LLMs) and deep learning models directly on edge workstations and local cluster environments.

This documentation serves as a comprehensive system manual for setting up, configuring, and optimizing cluaiz nodes.

---

## 🏛️ System Philosophy: Decoupled Edge Architecture

The framework enforces absolute separation of concerns across its core execution layers:

* **Sovereign Client Layer (`cluaiz-cli`):** The terminal interface and interactive dashboard built in Rust using Ratatui. Communicates asynchronously via local network sockets or named pipes.
* **Core Engine Daemon (`cluaiz-engine` / `cluaizdb`):** The orchestration center built with the Axum web framework. Manages model lifecycles, configuration states, and FFI pipelines.
* **Silicon Driver Bridges:** Dynamically linked binaries targeting SIMD CPU (AVX-512, ARM Neon) or discrete GPU (CUDA, Metal, Vulkan) cores.

---

## 🔒 Local Isolation & Data Protection

cluaiz prioritizes absolute privacy by routing all computing operations on-device:

* **On-Device Embedding Vectors:** All document chunking and vector space creations occur inside local memory allocations.
* **Deterministic Sandboxing:** Sandboxed WASM routines run under strict memory limits and fuel restrictions to isolate third-party extensions.
* **Controlled Telemetry:** Diagnostics tracking is disabled by default and runs entirely under explicit user toggle parameters.
