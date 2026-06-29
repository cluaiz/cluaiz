# 🧠 Neural Core (`interface-engines/neural_core/`)

<p align="center"><strong>The Mathematical Representation Layer</strong></p>

---

## 🎯 Deep Purpose

The `neural_core/` crate acts as the intermediate mathematical translation layer. When a `.gguf` file or an `.onnx` file is loaded from the disk, the physical layout of the tensors is drastically different. The Neural Core provides a unified Rust representation (the "Structural DNA") of neural networks. 

It defines exactly what a "Tensor" or a "KV Cache" is in the context of the cluaiz Engine, allowing the rest of the application to manipulate neural memory without understanding the specific C++ backend constraints.

## 🏛️ Architectural Mechanics

```mermaid
graph LR
    Disk["Model File (.gguf)"] --> Backend["llama/ (C++ Reader)"]
    Backend -->|"Translates to Native"| NeuralCore["neural_core/ (Rust Tensors)"]
    NeuralCore -->|"Provides Safe Access"| Engine["Core Inference Loop"]
```

## 🧬 Significant Details
- **The Core Logic:** Defines generic Tensor structs and matrix multiplication traits.
- **The "Why":** Without this crate, the main execution loop would be littered with `if backend == llama { ... } else { ... }` blocks. The Neural Core forces all backends to conform to a single, mathematically rigorous Rust interface.
