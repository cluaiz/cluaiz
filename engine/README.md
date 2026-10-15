# ⚙️ Cluaiz Compute Subsystem (`engine/`)

The direct, hardware-agnostic neural inference subsystem for Cluaiz. Contains the single source of truth for hardware metrics, planning, models, and execution backends.

## 📐 Architecture & Data Flow

```mermaid
graph TD
    Client[REST / SSE / CLI] --> API[api/: Axum Server]
    API --> Runtime[engine/runtime: Model Host & Lifecycle]
    Runtime --> Plan[engine/plan: Placement Planner]
    Plan --> HW[engine/hardware: Live HW Truth & NVML]
    Plan --> Models[engine/models: GGUF Reader & Vault Registry]
    Plan --> Core[engine/core: Settings Snapshot & Paths]
    Runtime --> Llama[engine/llama: Custom Llama Fork rlib]
    Runtime --> ONNX[engine/onnx: Whisper STT & Kokoro TTS]
```

## 🗂️ Sub-crates & Directories

| Subsystem | Crate Name | Description |
|---|---|---|
| [`core/`](core/) | `cluaiz-core` | Environment paths, RAM-cached settings, units (`GiB`/`MiB`), and engine traits. |
| [`media/`](media/) | `cluaiz-media` | Bounded local/HTTP/inline sources, image/audio decoding and explicit PCM preparation. ONNX consumes the common decoders; typed API/llama delivery remains pending. Core owns decoded-buffer/settings contracts. |
| [`hardware/`](hardware/) | `cluaiz-hw` | Direct hardware truth, NVML GPU probing, CPU/RAM discovery. |
| [`models/`](models/) | `cluaiz-models` | HuggingFace downloader, model vault registry, and single GGUF reader with SWA math. |
| [`plan/`](plan/) | `cluaiz-plan` | Pure mathematical placement planner (Candidates A-F, budgets, and context window). |
| [`llama/`](llama/) | `llama-engine` | Safe Rust wrapper linking the custom llama.cpp fork directly as an `rlib`. |
| [`onnx/`](onnx/) | `onnx-engine` | Multi-modal audio (Whisper STT, Kokoro TTS) and Vision processing. |
| [`runtime/`](runtime/) | `cluaiz-runtime` | Single unified engine host replacing legacy dispatcher and routers. |
