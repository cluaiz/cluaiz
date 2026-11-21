# System Architecture Overview

One program, `cluaiz`, holds the CLI, the terminal agent, the HTTP server and the model runtimes.

---

## Layers

![The Engine in four layers: api, cmd and automation on top; runtime and agents; plan, llama, onnx and media; models, hardware and core at the bottom. A crate depends only on crates below it](../assets/engine/crate-layers.png)

![One chat request: api, runtime ensure_loaded, hardware free memory, plan, llama load, stream, answer with timings](../assets/engine/chat-request.png)


```mermaid
flowchart TD
    CLI["cluaiz CLI and terminal agent (cmd/)"]
    API["HTTP server: cluaiz serve (api/)<br/>/v1/chat/completions, /v1/cep, /mcp, …"]
    FFI["C library cluaiz_cep.dll (ffi/)<br/>source only, not in release packages"]
    HOST["EngineHost (engine/runtime)<br/>model load, KV cache, memory sharing"]
    PLAN["Planner (engine/plan)<br/>GPU / RAM placement"]
    LLAMA["llama.cpp (engine/llama)<br/>GGUF"]
    ONNX["ONNX Runtime (engine/onnx)<br/>TTS, STT, embeddings"]
    GPU["GPU backend libraries<br/>CUDA / Vulkan / Metal"]

    CLI --> HOST
    API --> HOST
    FFI --> API
    HOST --> PLAN
    HOST --> LLAMA
    HOST --> ONNX
    LLAMA --> GPU
```

### CLI and terminal agent (`cmd/`)
* `cluaiz run` chats in the terminal; `cluaiz agent` reads, searches and edits files in a folder
  and runs commands, asking before changes.
* The CLI loads models in its own process; it does not need a running server.

### HTTP server (`api/`)
* Axum and Tokio. Chat streams with Server-Sent Events.
* Builds each chat prompt from history, recall, notes, project files and tool cards, then runs
  the tool loop.
* Load settings come from `settings.json` → `optimization` ([load and speed
  settings](../engine/llm_optimization.md)).

### Runtimes and GPU backends
* At start, llama.cpp loads the GPU backend libraries found next to the executable
  (`ggml_backend_load_all_from_path`). Without one, it runs on the CPU (AVX2, AVX-512 or NEON).
* The planner reads the [hardware profile](../engine/system_control.md) and the model size and
  chooses the GPU layers and context size.

More: [ARCHITECTURE.md](../../ARCHITECTURE.md).
