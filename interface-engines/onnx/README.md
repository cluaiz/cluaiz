# 💠 ONNX Backend (`interface-engines/onnx/`)

<p align="center"><strong>The ONNX Runtime Execution Engine</strong></p>

---

## 🎯 Deep Purpose

The `onnx/` crate provides execution support for the Open Neural Network Exchange (`.onnx`) format via Microsoft's ONNX Runtime. This backend is primarily used for deterministic, highly optimized enterprise models or specific Vision/Audio models that do not fit into the GGUF text-generation paradigm.

## 🏛️ Architectural Mechanics
- **The Core Logic:** Connects the `dispatcher` to the `libonnxruntime` C-API, mapping cluaiz inputs into ONNX execution graphs.
- **The "Why":** While GGUF is excellent for quantized LLMs, ONNX provides superior cross-platform execution for specialized embedding models and standard deep learning classifiers.

## 🚀 System Booster vs ONNX Reality

When the Core Engine loads an ONNX model, it passes the `system_booster.json` configuration via the `cluaizBoosterContext` FFI struct. However, while the `cluaiz-llama` backend maps these parameters 1-to-1 dynamically, **ONNX behaves differently due to its static computational graph architecture**.

Developers MUST understand this reality to prevent false expectations:

### 1. What is Manually Mapped (Respected)?
- **GPU Layers (`n_gpu_layers`)**: Fully mapped to ONNX Execution Providers.
  - `-1`: **Auto Telemetry** (Scans VRAM/RAM dynamically. Routes to CUDA if safe, falls back to CPU if under pressure).
  - `0`: **Force CPU** (Strictly uses AVX2/AVX512 execution provider).
  - `>0`: **Force GPU** (Strictly forces CUDA Execution Provider, ignoring telemetry warnings).

### 2. What is Automatically Baked (Ignored from Booster)?
ONNX Runtime does not accept dynamic runtime toggles for deep memory features because they are pre-compiled into the `.onnx` file itself:

- **Flash Attention (`flash_attention`)**: Cannot be toggled manually via `system_booster.json`. If you run ONNX with CUDA (GPU), Flash Attention is triggered *automatically* by the C++ backend if your hardware supports it. There is no manual ON/OFF switch.
- **Dynamic Quantization (`turbo_quant`)**: Unlike `llama.cpp`, ONNX models cannot be compressed from 32-bit to 4-bit at load time efficiently. You MUST download a **pre-quantized** ONNX model (e.g., `model-int4.onnx`).
- **KV Cache Quantization (`kv_cache_quantization`)**: Implementing CPU-side f32 to i8 compression for every single token causes massive latency. Therefore, ONNX natively holds the KV cache in the exact datatype requested by the model graph (`f32` or `f16`) for maximum *blazing fast* generation speed.
