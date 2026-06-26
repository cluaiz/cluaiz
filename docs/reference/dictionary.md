# Cluaize System Dictionary

A technical glossary defining terms, strategies, and technologies used across the Cluaize Inference Engine codebase.

---

## 📖 Glossary

* ### **CDQL (Cluaize Database Query Language)**
  The specialized query vocabulary mapped over CEL syntax to perform structured database operations inside `cluaizd`.
* ### **CEL (Cluaiz Engine Language / Common Expression Language)**
  The core orchestration DSL (Domain-Specific Language) parsed into AST instructions to route tasks across local sandboxes.
* ### **Dynamic Silicon Dispatch**
  The decoupled driver loading strategy where dynamic compute libraries (such as `.dll` files for CUDA, Vulkan, or Metal) are bound at runtime based on hardware scans.
* ### **Flash Attention**
  An optimized Transformer attention mechanism that performs tiling on GPU SRAM to bypass expensive Global Memory round-trips.
* ### **GGUF**
  The single-file binary model format used to store compressed, quantized neural weights for CPU/GPU execution.
* ### **KV Cache**
  The memory buffer storing Key and Value matrix histories for processed tokens to avoid redundant calculations during conversational loops.
* ### **PCIe Spill**
  A performance bottleneck occurring when models exceed VRAM bounds, causing the Operating System to swap parameters over the slow PCIe interface.
* ### **Speculative Decoding**
  A generation strategy executing a small draft model to hallucinate candidate tokens, which are verified in batches by the target model.
* ### **VRAM Arbiter**
  The memory scheduler managing loading, pinning, and swap actions for neural networks targeting graphics accelerators.
