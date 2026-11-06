# Introduction to Cluaiz

Cluaiz is a local AI engine. It runs language, speech and embedding models on your own computer,
with the files and memory staying on your disk.

This documentation covers installing the Engine, running models, the terminal agent, the HTTP API
and CEP (the Engine's JSON-RPC protocol).

---

## What it is made of

* **`cluaiz` program:** the CLI (`run`, `agent`, `pull`, `serve`, …), the terminal coding agent
  and the HTTP server, in one executable.
* **Two model runtimes:** llama.cpp for GGUF models and ONNX Runtime for ONNX models (TTS, STT,
  embeddings). See [the two runtimes](../engine/dual_engine_architecture.md).
* **GPU packs:** the CPU build works everywhere; CUDA 12, CUDA 13 and Vulkan packs add GPU support
  (Metal is built into the macOS build).
* **Ways in:** the CLI, the HTTP API (`cluaiz serve`), a C library (`cluaiz_cep.dll`) and MCP.

The full map is in [ARCHITECTURE.md](../../ARCHITECTURE.md).

---

## Local data

* Models, chat memory, KV caches, notes and settings are stored under `~/.cluaiz` (or
  `CLUAIZ_HOME`). See the [install layout](../deployments/install-layout.md).
* Documents are read and chunked in memory by `/v1/ingest`; there is no vector database.
* Telemetry in chat responses (`stream_telemetry`) is off by default.
* WebAssembly plugins run closed in, with memory and instruction limits. Other plugins run as normal child processes with your user's rights, so install
  only plugins you trust. MCP servers and tools ask for approval according to the
  `agent_security_mode` in [permission.json](../engine/permission.md).
