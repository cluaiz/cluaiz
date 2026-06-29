# cluaiz System Booster - Deep Technical Master Guide

The `system_booster.json` file is the central nervous system of the cluaiz Inference Engine. It dictates the lowest-level hardware interactions—how neural network weights map to physical silicon (RAM/VRAM), how OS-level memory pages are managed, and how computation graphs are executed by the CPU/GPU. 

This guide provides an exhaustive, deep dive into every single parameter, its underlying mathematical mechanism, UI mappings, and the absolute consequences of misconfiguration.

---

## 1. Compute Device / GPU Offload (`n_gpu_layers`)

This is the most critical pillar connecting the `llama.cpp` backend to the hardware. It determines how the Transformer blocks (layers) of a model are split between System RAM (CPU) and Video RAM (GPU). In the UI, this maps to the "Select Compute Device" menu.

### A. GPU (Full Offload) `[Value: -1]`
* **Deep Mechanism:** The engine attempts to convert all transformer layers into CUDA/Metal tensors and forces the entire allocation onto the GPU's VRAM. 
* **The PCIe Bottleneck Risk:** If the model size exceeds physical VRAM (e.g., loading a 10GB DeepSeek model onto a 4GB RTX 3050), Windows/Linux will utilize "Shared GPU Memory". The OS will fill the 4GB VRAM and dump the remaining 6GB into System RAM. Because the computation target is still forced to the GPU, for *every single token generated*, 6GB of tensor data must travel across the PCIe Gen3/Gen4 bus from RAM to GPU, be processed, and return.
* **Result of Misconfiguration:** Generation speed will catastrophically plunge to 1-2 TPS. The engine may throw `invalid vector subscript` C++ out-of-bounds exceptions or CUDA `cudaErrorMemoryAllocation` during the massive contiguous `malloc` attempts.
* **Golden Rule:** ONLY select Full Offload if `Total Model Size in GB < Total Physical VRAM`.

### B. CPU Only `[Value: 0]`
* **Deep Mechanism:** Completely bypasses the GPU. All weights are `mmap`ped into System RAM. The computation graph is executed entirely on the CPU utilizing vector instruction sets like AVX2, AVX-512, or ARM NEON. 
* **The Bandwidth Advantage:** While a CPU has fewer cores than a GPU, it has direct, zero-latency access to System RAM (DDR4/DDR5) via the motherboard's memory controller. 
* **Golden Rule:** If you are running a massive 32B model on a low-VRAM system, **CPU Only** will dramatically outperform "GPU Full Offload". The CPU will generate at a steady 5-8 TPS, completely avoiding the PCIe transfer bottleneck that kills performance in Shared GPU Memory mode.

### C. Custom Layers / Hybrid `[Value: X, e.g., 15]`
* **Deep Mechanism:** The smartest mode for memory-constrained systems. If a model has 40 layers, and you set this to `15`, the backend will surgically pin the first 15 layers into VRAM and map the remaining 25 into System RAM.
* **Execution Flow:** During the forward pass for a token, the CPU processes the first 25 layers in RAM. The intermediate output (a tiny multi-megabyte tensor) is then sent over the PCIe bus *exactly once* to the GPU, which finishes the remaining 15 layers in VRAM.
* **Golden Rule:** This mode eliminates continuous PCIe swapping. It is the absolute best option for running models larger than your VRAM. You must experiment to find the maximum `X` that fits in your VRAM without spilling into Shared Memory.

---

## 2. Speculative Decoding / Assisted Generation (`speculative_decoding`)

* **Deep Mechanism:** Standard autoregressive generation computes one token at a time. Speculative decoding instantiates a completely separate, tiny "Draft Model" (e.g., 1B parameters) alongside your main "Target Model" (e.g., 32B). The draft model hallucinates 4-5 tokens instantly. The main model then takes these 5 tokens and processes them in a *single forward pass batch*. If the draft tokens match what the main model would have produced, all 5 are accepted simultaneously, yielding a 2x-4x speedup.
* **VRAM Penalty:** You are paying for two models. You must have enough VRAM to hold the Target Model, the Draft Model, and *two* separate KV caches.
* **Golden Rule:** Enable this on high-end hardware (12GB+ VRAM) for instantaneous coding tasks. 
* **Danger Zone:** 
    * If you have 4GB VRAM, enabling this will trigger an immediate OOM crash or push you into fatal PCIe swapping.
    * **Architectural Incompatibility:** Speculative decoding relies on exact Transformer topologies. If you load a Hybrid/Mamba/SSM (State Space Model) or an RNN (like RWKV), this feature will cause fatal segmentation faults because the draft tree verification algorithms cannot map attention heads. It must be strictly `"Off"` for these architectures.

---
## 3. Force Memory Lock / mlock / Paging Lock (`force_memory_lock`)

* **Deep Mechanism:** In normal OS operations (Windows/Linux), if RAM gets full, the OS takes idle background applications and dumps their memory into the Hard Drive's Pagefile/Swap to free up space. If you pause chatting with the AI for 10 minutes, the OS might page out the model. When you send a new message, the system freezes for 10 seconds while gigabytes of weights are dragged back from the SSD into RAM. 
* **The Lock:** Setting this to `"On"` triggers the POSIX `mlock()` or Windows `VirtualLock()` system calls. It explicitly commands the OS kernel: *Never, under any circumstances, page this memory to disk.*
* **Options:**
    * `"Auto"`: Enables only if physical RAM is significantly larger than the model.
    * `"On"`: Forcefully locks the mapped memory.
    * `"Off"`: Allows the OS to page memory normally.
* **Danger Zone:** If your system RAM is exactly 16GB, and you load a 14GB model, turning this `"On"` will cause the OS to panic (Out of Memory), crashing the engine or instantly bluescreening Windows because it cannot swap idle memory to disk.

---

## 4. Flash Attention / SDPA (`flash_attention`)

* **Deep Mechanism:** Standard Self-Attention requires computing a massive $N \times N$ matrix (where N is context length) and writing it to the GPU's Global High-Bandwidth Memory (HBM). For a 32,000 context window, this matrix alone can consume gigabytes of VRAM. FlashAttention uses "tiling"—it computes small chunks of the attention matrix entirely inside the GPU's ultra-fast, tiny SRAM (L1/L2 cache) and updates the final result without ever writing the intermediate $O(N^2)$ matrix to VRAM.
* **Options:**
    * `"On"`: Heavily recommended. Saves massive VRAM and speeds up processing.
    * `"Off"`: Use only for debugging older hardware compatibility issues.

---

## 5. KV Cache Quantization / Context Compression (`kv_cache_quantization`)

* **Deep Mechanism:** Every word (token) in the chat history must be mathematically stored in the Key-Value (KV) cache so the model remembers it. By default, this is stored in FP16 (16-bit precision). If you paste 20,000 lines of code, the KV cache inflates massively, often causing OOM crashes even if the model weights fit perfectly.
* **Options:**
    * `"Auto"`: Engine automatically selects quantization based on available VRAM vs Context Size.
    * `"Kv16"`: Default 16-bit FP. Maximum accuracy, massive memory footprint.
    * `"Kv8"`: 8-bit precision. Cuts history VRAM footprint by 50% with almost 0 performance loss.
    * `"Kv4"`: 4-bit precision. Cuts history VRAM footprint by 75%. Essential for 128K context lengths on standard 24GB GPUs.

---

## 6. Context Shifting / KV Cache Rolling (`context_shifting`)

* **Deep Mechanism:** When the model reaches its maximum context limit (e.g., 8192 tokens), traditional engines crash with `LLAMA_ERROR_CONTEXT_FULL` and force you to clear the chat. Context Shifting intercepts the KV cache just before overflow. It surgically drops the oldest 50% of the conversational history (tokens 500 to 4096), shifts the recent history upward, but crucially **preserves the System Prompt (tokens 0 to 500)** at the very top.
* **Options:**
    * `"Auto"`: Dynamically handles shifting (Defaults to Standard/10%).
    * `"Off"`: Disabled. Engine will crash/stop on overflow.
    * `"Minimal"`: Drops oldest 5% of tokens safely.
    * `"Standard"`: Drops oldest 10% of tokens safely.
    * `"Aggressive"`: Drops oldest 25% of tokens.
    * `"Extreme"`: Drops oldest 50% of tokens.

---

## 7. Force VRAM Reclaim / Aggressive Garbage Collection (`force_vram_reclaim`)

* **Deep Mechanism:** Modern inference engines use memory pools. When a buffer is no longer needed for a computation, it isn't deleted; it's kept in a pool to be reused for the next token, saving CPU allocation overhead. `force_vram_reclaim` destroys the pool. It acts as a ruthless landlord, forcing the engine to `free()` every single tensor the microsecond the computation finishes.
* **Options:**
    * `"Off"`: Default. Allows memory pools for maximum speed.
    * `"On"`: Slashes VRAM usage, but adds ~5-10% CPU overhead due to constant memory alloc/dealloc cycles.

---

## 8. Neural Mode / Concurrency Scheduler (`mode_run`)

* **Deep Mechanism:** Determines thread yielding and CPU spin-locks.
* **Options:**
    * `"edge"`: Mobile/NPU/Pi (Extreme pruning).
    * `"multitasking"`: Standard Laptop (Respects OS/Apps).
    * `"balance"`: Normal behavior / Standard Performance.
    * `"max_boost"`: Sets CPU threads to real-time priority, halting background Windows tasks.
    * `"ultra_max_boost"`: Reclaims everything (Formerly Landlord).
    * `"hyper_cluster"`: Server/H100 Cluster (Zero-margin orchestration).

---

## 9. Turbo Quant / Tensor Core Compensator (`turbo_quant`)

* **Deep Mechanism:** Neural networks rely heavily on precision arithmetic (FP16/FP32). When you load a heavily quantized model (like a 3-bit Q3_K_M), the weights are drastically compressed to save VRAM, but this introduces significant mathematical "noise" and decorrelation in the weight matrices. `turbo_quant` acts as an advanced mathematical compensator. Before the actual matrix multiplication occurs in the CPU/GPU SRAM, it applies rapid corrections (such as Hadamard transforms and Givens rotations) to mathematically reverse some of the quantization damage. 
* **Options:**
    * `"On"`: Boosts logic and coding accuracy on highly compressed models (< 4 bits/weight).
    * `"Off"`: Disables correction.

---

## 10. Auto Round / Dynamic Precision Rounding (`auto_round`)

* **Deep Mechanism:** This is a complementary precision math injector that works alongside `turbo_quant`. While quantization aggressively rounds continuous floating-point numbers to discrete integers, `auto_round` dynamically calculates the optimal rounding scheme (like stochastic rounding or nearest-even) for the weight tensors during the forward pass. This dynamically shifts the rounding threshold to minimize activation outliers that usually destroy the coherence of small models.
* **Options:**
    * `"On"`: Engages stochastic dynamic rounding for precision.
    * `"Off"`: Standard nearest-neighbor rounding.

---

## 11. DFlash / Dynamic Flash Attention (`dflash`)

* **Deep Mechanism:** Standard Flash Attention (introduced in Section 6) drastically reduces VRAM usage by tiling the attention matrix. However, it typically pre-allocates a maximum fixed chunk of SRAM based on the absolute maximum context length you configure. `dflash` (Dynamic Flash Attention / FlashKDA) takes this a step further. It dynamically monitors the exact token length of your real-time conversation and only allocates the precise amount of SRAM needed at that exact microsecond, scaling it up dynamically as the chat grows.
* **Options:**
    * `"Auto"`: Dynamically adjusts SRAM buffer size frame-by-frame.
    * `"Off"`: Static maximal allocation.

---

## 12. Think Mode / Chain of Thought (CoT) (`think_mode`)

* **Deep Mechanism:** Traditional generation models immediately output the first word that comes to their neural pathways. `think_mode` intercepts this. Before outputting the final token, it forces the model to engage in an internal, hidden "Chain of Thought" (CoT) reasoning phase. The model generates tokens into a hidden buffer, evaluates its own logic, self-corrects, and only when it mathematically concludes it has the right approach, does it stream the final output to the UI.
* **Options:**
    * `"Auto"`: Only activates when the system prompt detects complex logical queries.
    * `"On"`: Forces CoT for every single prompt.
    * `"Off"`: Completely disables the internal reasoning path. The model acts as a pure autoregressive generator.
* **Golden Rule:** Set to `"Auto"`. This gives you the best of both worlds—instant responses for basic chat, and deep reasoning for complex debugging tasks.
* **Danger Zone:** Setting this to `"On"` for general conversation will result in extreme latency. The model might take 10-15 seconds to internally "think" about how to respond to a simple "Good morning", creating a terrible user experience.

---

## 13. Response Length / Max Tokens Limit (`response_length`)

* **Deep Mechanism:** By default, language models generate tokens until they emit a specialized `<|end_of_text|>` or EOS (End of Sequence) token. However, for programmatic applications, you often need to cap the absolute maximum compute spent on a single generation cycle. `response_length` acts as a hard mathematical ceiling in the generation loop. If the token count hits this integer, the engine forces an EOS token injection, instantly halting computation and saving compute resources.
* **Options:**
    * `"auto"`: Default behavior. The model decides when it is finished speaking based on its training.
    * `"short"`: Forces concise replies and rapid EOS generation.
    * `"long"`: Encourages exhaustive answers.
    * `"<Integer>"` (e.g., `"500"`): Enforces a strict ceiling of exactly 500 tokens per response.
* **Golden Rule:** Leave this blank `""` for normal usage so the AI can fully elaborate on coding solutions.
* **Danger Zone:** If you set this to a low number (like `100`), the model will frequently get cut off mid-sentence or mid-code-block. This breaks the UI parsing and leaves you with half-written, unusable code.

---

## 14. JSON Mode / Structured Output (`enforce_json`)

* **Deep Mechanism:** Language models naturally generate unstructured human text. When building API endpoints or automated agents, you need strict, parsable JSON. `enforce_json` engages a "Logit Processor". At every single token generation step, it scans the model's vocabulary and masks out (sets probability to zero) any token that would result in syntactically invalid JSON strings, arrays, or objects. It physically forces the neural network down a path where only valid JSON syntax is possible.
* **Options:**
    * `true`: Engages the Logit Processor for strict JSON validation.
    * `false`: Normal, unrestricted text generation.
* **Golden Rule:** Only enable `true` when cluaiz is being operated headlessly as a backend API for other software that requires mechanical parsing of the output.
* **Danger Zone:** Never, under any circumstances, enable this for general chatting in the UI. If you say "How are you?", the model cannot output normal text. It will mathematically struggle to force a conversational reply into a rigid JSON structure, which typically results in total engine crashes, infinite loops, or absolute gibberish outputs.

---

## 15. The "Llama vs ONNX" Architectural Reality

It is a critical engineering fact that `system_booster.json` maps differently depending on which interface engine is actively executing the neural graph. 

The `cluaiz-llama` crate is a **dynamic engine**. It loads `.gguf` files and can dynamically reshape the memory, quantize tensors on the fly, and toggle advanced attention algorithms (like Flash Attention) at runtime based on the JSON settings.

The `cluaiz-onnx` crate is a **static engine**. It loads `.onnx` files, which are highly optimized, pre-compiled computational graphs. 
Because of this fundamental architectural difference, ONNX ignores several `system_booster.json` parameters in favor of automated backend optimization:

* **GPU Override (`n_gpu_layers`)**: This is the **ONLY** setting fully respected by ONNX. It maps `0` to CPU (AVX) and `>0` to GPU (CUDA). Setting `-1` triggers the auto-telemetry VRAM scanner.
* **Flash Attention (`flash_attention`)**: ONNX ignores the manual ON/OFF toggle. If you route the model to CUDA, ONNX will *automatically* trigger Flash Attention internally if the hardware supports it.
* **Dynamic / KV Quantization (`turbo_quant`, `kv_cache_quantization`)**: ONNX ignores these. Because ONNX graphs are static, they cannot be compressed at load time without extreme CPU latency. To get 4-bit quantization in ONNX, you must download a **pre-quantized** model (e.g., `model-int4.onnx`). For the KV Cache, ONNX natively locks it to the exact data type (`f32` or `f16`) requested by the model graph to guarantee blazing fast (makkhan) generation speeds.

*This document serves as the absolute source of truth for cluaiz Engine hardware manipulation. No configuration should be altered in production without explicitly understanding the ramifications documented above.*
