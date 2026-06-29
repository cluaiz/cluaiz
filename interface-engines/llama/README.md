# 🦙 Llama Backend (`interface-engines/llama/`)

<p align="center"><strong>The GGUF / Llama.cpp Execution Engine</strong></p>

---

## 🎯 Deep Purpose

The `llama/` crate is the dedicated backend runner for the `.gguf` model format. It leverages the highly optimized `llama.cpp` (or `llama.rs`) math kernels to perform integer-quantized matrix multiplications on Apple Silicon, CUDA GPUs, and consumer CPUs.

## 🏛️ Architectural Mechanics
- **The Core Logic:** Binds the engine's `neural_core/` structs to the specific functions required to execute a GGUF file. It manages the specific KV Cache allocation strategies unique to the Llama architecture.
- **The "Why":** GGUF is the industry standard for highly quantized edge models. This crate ensures cluaiz has native, zero-overhead execution paths for the most popular models in the world.
