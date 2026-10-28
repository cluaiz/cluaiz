<p align="center">
  <picture>
    <source media="" srcset="assets/Banner.png">
    <img alt="Cluaiz Banner" src="assets/Banner.png" width="100%">
  </picture>
</p>
<h3 align="center">
Power your local AI and execute native inference with bare-metal hardware and real-time LLM steering using modular plugins, MCP connectors, skills, and one protocol (CEP) for every engine operation</h3>

</br>
<p align="center">
  <img src="https://img.shields.io/badge/Status-Alpha-orange?style=for-the-badge" alt="Status">
  <img src="https://img.shields.io/badge/Logic-Native_FFI-green?style=for-the-badge" alt="Logic">
  <img src="https://img.shields.io/badge/Architecture-Modular-blue?style=for-the-badge" alt="Architecture">
  <img src="https://img.shields.io/badge/Security-Sandboxed-red?style=for-the-badge" alt="Security">
   <br>
  <img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=for-the-badge" alt="License">
  <img src="https://img.shields.io/badge/Built_with-Rust-e43716.svg?style=for-the-badge&logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Win_|_Mac_|_Linux-lightgrey?style=for-the-badge" alt="Platform">
  <img src="https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=for-the-badge" alt="PRs Welcome">
</p>

<p align="center">
  <a href="https://cluaiz.com/docs"><b>Documentation</b></a> &nbsp;|&nbsp;
  <a href="https://cluaiz.com"><b>Website</b></a> &nbsp;|&nbsp;
  <a href="https://discord.gg/mab8kBURz"><b>Discord</b></a> &nbsp;|&nbsp;
  <a href="https://reddit.com/u/cluaiz"><b>Reddit</b></a> &nbsp;|&nbsp;
  <a href="https://linkedin.com/company/cluaiz"><b>LinkedIn</b></a>
</p>

## About Cluaiz

<h4>Cluaiz is a fast, easy-to-use, unified native runtime engine and orchestrator built in Rust. Optimized for edge devices, it powers local AI and agentic workflows with one engine protocol (CEP), WASM, and modular plugin support.</h4>

Cluaiz is a high-throughput, Rust-based inference orchestrator and runtime engine meticulously engineered to synchronize text, vision, and tool execution into a single unified memory space. This architecture is not bound to specific hardware configurations or constrained environments; it is designed as a highly elastic architecture to operate seamlessly across any tier of compute hardware—from consumer silicon to enterprise server clusters. Its primary objective is to establish a direct, transparent, and highly optimized bridge between mathematical inference kernels and autonomous agentic workflows, granting developers absolute programmatic authority and optimized execution.

- **The Hybrid Access and Connectivity Model**: Rather than restricting developers to a single protocol, Cluaiz introduces a comprehensive hybrid access paradigm designed to adapt to any architectural requirement. If a decoupled web application requires standard network requests, the engine's built-in HTTP REST API gateway provides a robust solution. Engine control operations are methods of CEP (Cluaiz Engine Protocol), JSON-RPC 2.0: the same call works over HTTP (`POST /v1/cep`), over MCP (`/mcp`, `cluaiz mcp-server`) and through the in-process C library (`cep_call` / `cep_free`) for apps that embed the Engine, alongside direct Chat, TTS, STT, Embeddings and Ingest HTTP routes and client compatibility endpoints. Plugins can run as WebAssembly (WASM) or native libraries.

- **Dual-Engine Orchestration and Multimodal Integration**: Cluaiz is not a superficial wrapper; it natively orchestrates the execution of underlying mathematical kernels. It leverages the core logic of `llama.cpp` to ensure highly efficient parsing of GGUF models and broad cross-platform hardware portability. Simultaneously, the `ONNX Runtime` is deeply integrated to execute multimodal operations—such as vision, embeddings, and audio—in parallel. By synchronizing these engines, Cluaiz enables hybrid CPU and GPU split-loading, allowing dynamic context switching between text and vision operations without losing the underlying latent states.

- **Compile-Time 1-Bit BitNet Injection & Cross-Platform Execution**: Cluaiz optimizes the execution of 1-bit and 1.58-bit (Ternary) quantization models through dynamic compile-time interception. By leveraging the cross-platform hardware portability of `llama.cpp`, the engine ensures that BitNet models can run natively on any hardware tier (CPU or GPU). During the build process, Cluaiz dynamically patches the underlying C++ inference libraries, hijacking the tensor math operations before they compile. Through injected low-level hardware instructions, the engine forces both CPUs and GPUs to process the model using actual 1-bit calculations natively. By executing pure addition and subtraction directly on hardware registers, Cluaiz achieves true 1-bit primitive throughput and entirely avoids floating-point conversions across all devices.

- **State Manipulation, Prefix Reuse, and Memory Safety**: The engine provides complete, transparent oversight over system operations. It features native KV-Cache Prefix Reuse and zero-recompute tool resumption, allowing token generation to be paused mid-stream (via CLI Ctrl+C or API tool execution) so that custom skills or runtime plugins can be executed under strict Rust safe memory isolation. Generation then instantly resumes from active context via `[PIVOT_CONTINUE]` without requiring full re-prompt token recalculation. Parallel to this, the Memory Governor continuously monitors physical memory headroom, actively preventing Out-of-Memory (OOM) crashes across hardware targets.

- **Engine Optimization & Memory Governance**: Cluaiz features a dedicated optimization profile granting developers surgical tunability over runtime behaviors. It dictates memory allocation boundaries, dynamic hardware-layer splits, and advanced KV-cache compression techniques (such as `kv_cache_quantization` and rolling `context_shifting`) directly governed by the Memory Governor. Additionally, it unlocks advanced execution features like Flash Attention, MoE SSD Streaming, Speculative Decoding, and Hybrid Memory allocation, allowing users to precisely balance execution speed against memory limitations.

- **Integrated Developer Hub**: To manage this vast ecosystem, Cluaiz includes a highly capable local Developer Hub operating on a secure local port. Here, developers are granted access to the complete API matrix, enabling them to live-test their WASM plugins, ternary kernel designs, and execution pipelines. This sandboxed environment provides the freedom to test, optimize, and push the engine's boundaries over HTTP, MCP or the C library.

## 🛡️ **Project Trust & Current Status**

> [!WARNING]
> **Project Current State**: This project is currently undergoing rigorous testing for the upcoming Alpha release.
> You should expect significant restructuring, breaking changes, and architectural shifts.
>
> **Current Phase: Pre-Alpha Validation**.
> We are actively running benchmark testing, code quality evaluations, and hardware validation. The stable Alpha version will be available soon. Until then, the repository is subject to heavy modifications.

## 🚀 **Quick Start**

Start chatting instantly. No setup required. cluaiz handles all GGUF downloads and hardware compilation natively.

### 1. Install cluaiz

**Windows (PowerShell)**

```powershell
powershell -c "irm https://cluaiz.com/install.ps1 | iex"
```

**Linux & macOS (Shell)**

```bash
curl -fsSL https://cluaiz.com/install.sh | bash
```

### 2. Launch the Interactive TUI Dashboard

Run the naked `cluaiz` command to launch our full-terminal interactive control panel:

```bash
cluaiz
```

### 3. Start the Background API Server

Run the background daemon to serve models via the OpenAI-compatible REST API:

```bash
cluaiz serve
```

> [!TIP]
> **Pure Client Auto-Detection**: If you start `cluaiz serve` in the background, and then run the `cluaiz` dashboard in another terminal, the dashboard will automatically detect the running server and connect to it as a **Pure Client**. It does not load a second engine, so the model sits in VRAM only once.

### 4. Direct Headless Inference

Run any locally cached model by name:

```bash
cluaiz run gemma2:2b
```

Or pass a full **HuggingFace repo ID** — cluaiz will automatically download the GGUF weights and run inference:

```bash
# Syntax
$ cluaiz run <id>

# Example
$ cluaiz run Qwen/Qwen3-VL-2B-Instruct-GGUF
```

> [!NOTE]
> HuggingFace downloads are handled natively. cluaiz fetches GGUF weights directly over HTTPS and caches them under `~/.cluaiz/models/`.

### 5. Install Skills & Plugins Natively

Extend the AI's capabilities natively:

```bash
# Install a new skill by name
$ cluaiz skill install github-assistant

# Install a plugin
$ cluaiz plugin install web-scraper
```

---

## 📚 **Deep Documentation Reference**

<details open>
<summary><b>Click to expand full technical documentation</b></summary>

### 1. Terminal & CLI Core

- **[Terminal Commands & CLI Reference](docs/reference/terminal-commands.md)**  
  The `cluaiz` commands. It documents the exact execution flow of the kernel, internal JSON state mutations (like `Permission.json` updates), and the API each command maps to.
- **[Model Downloading & Vault Management](docs/get-started/model-downloading.md)**  
  Detailed architecture and usage guide for HuggingFace model downloading. Explains variant grouping, prefix-compatible scoped metadata filtering, local folder flattening, and GGUF/ONNX format capabilities.

### 2. Execution & Native Logic

- **[CEP (Cluaiz Engine Protocol)](docs/api/cep/README.md)**
  One JSON-RPC 2.0 endpoint for Engine controls (system, settings, models, files, tools, chat memory, notes, context, audio tasks, approvals), the same over HTTP, MCP and the in-process C library. Each method has its own page, in [five categories](docs/api/cep/README.md#the-methods); the payload block of every page is generated from the Engine's method registry.

### 3. Core Engine Architecture

- **[Dual Engine Architecture](docs/engine/dual_engine_architecture.md)**  
  Understand the heart of Cluaiz: the seamless orchestration layer. It explains how `cluaiz` acts as a unified C-level memory space bridging `llama.cpp` (for LLMs) and **ONNX Runtime** (for Vision & Embeddings), eliminating the fragmented network lag typical of multi-engine local AI setups.
- **[Prefix Caching Architecture](docs/engine/prefix_caching_architecture.md)**  
  Deep dive into the Prefix Caching pipeline. Learn how Cluaiz preserves dialogue context in memory and decodes only new delta tokens, drastically cutting Time To First Token (TTFT) and multi-turn prefill latency.
- **[Model Format & Architecture Release Matrix](RELEASE.md)**  
  Comprehensive support matrix defining supported package contracts, GGUF/ONNX compatibility across text, vision, audio (TTS/STT), and OCR modalities.

### 4. Hardware Governance & Security

- **[Engine Optimization & Hardware Tuning](docs/engine/llm_optimization.md)**  
  The guide to `llm_optimization.json` configurations. Discover how the Memory Governor reserves safety margins to prevent Out-of-Memory (OOM) crashes by configuring Flash Attention bounds, KV Cache Quantization (`kv4`, `kv8`, `kv16`), and Context Shifting sliding windows.
- **[Native Permission Schema](docs/engine/permission.md)**  
  A breakdown of the strict execution bounds. Explore how `Permission.json` acts as a bare-metal gatekeeper, controlling WASM Sandboxing strictness, active telemetry buffers, vectorization permissions, and native firewall rules to ensure absolute sovereign data security.

### 5. Unified Tool Governance & Live Context Telemetry

- **[Unified Tools API (`tools.list` & `chat.tools`)](developer_hub/data/tools.json)**  
  Single-endpoint architecture unifying Skills, WASM Plugins, and MCP bridges. Supports granular turn-based duration lifecycles (`turns: -1` persistent, `turns: 0` ephemeral single-message, `turns: N` multi-turn countdown).
- **[Live Context Window & KV-Cache Telemetry Engine](inference-engine/engines/core/src/telemetry)**  
  Real-time token breakdown tracking (Messages, System Prompt, Active Skills, WASM Plugins, MCP Tools, and Deferred Lazy Token Savings) with live mathematical KV-Cache VRAM and Process RSS RAM telemetry emitted over SSE.

</details>

---

## 🌍 **The Cluaiz Ecosystem**

<details open>
<summary><b>Click to explore the Cluaiz ecosystem</b></summary>

### 1. `cluaiz-app` (Official GUI)

- **[Repository: cluaiz-app](https://github.com/cluaiz/cluaiz-app)**
- **Technical Purpose:** This is the official graphical user interface. It connects as a **Pure Client** to the background `cluaiz serve` daemon.
- **Execution Flow:** By communicating with the engine over HTTP (`/health`, `/v1/chat/completions`, CEP `/v1/cep`), the app provides visual model management, vault inspection, and an interactive interface without duplicating the heavy inference engine in memory. This saves duplicate VRAM overhead.

### 2. `cluaiz-tools` (Global Registry & Plugins)

- **[Repository: cluaiz-tools](https://github.com/cluaiz/cluaiz-tools)**
- **Technical Purpose:** The central, automated manifest registry (`registry.json`) for the Cluaiz ecosystem. It hosts the source code and manifests for WASM Skills, Native Plugins, and MCP Servers. The CLI commands (e.g., `cluaiz plugin install`) directly fetch manifests from this tools repository to securely link plugins into the engine.
- **Example Plugin hosted in Tools:**
  - **[cluaiz-search](https://github.com/cluaiz/cluaiz-tools/tree/main/plugins/cluaiz-search):** A Native Dynamic Library (`cdylib`) built in pure Rust. It provides VRAM-aware web metasearch without heavy Python SDKs or Docker.
  - **Execution Flow:**
    1. The AI triggers an **Agentic Pause** natively mid-generation by emitting `<tool_call>{"name": "cluaiz-search", "arguments": {"query": "..."}}</tool_call>`.
    2. The plugin concurrently hits SearXNG and DuckDuckGo using `reqwest`, parses the DOM using `scraper` (stripping JS/CSS), and dynamically compresses the text (using BM25) to fit the available VRAM envelope.
    3. The parsed knowledge is injected directly into the active C-pointer KV-Cache and generation resumes seamlessly.

</details>

---

## 🧠 **Features & Capabilities**

<details open>
<summary><b>Click to expand</b></summary>

### 1. Hardware Execution & Memory Management

- **VRAM Arbiter (Memory Safety Margins):** Automatically probes physical hardware and enforces a strict memory safety margin. It ensures the system safely manages VRAM and RAM headroom to prevent Out-of-Memory (OOM) crashes across various GPU/CPU configurations.
- **Engine Optimization:** Through `llm_optimization.json`, gain deep hardware control over Flash Attention (`flash_attn`), KV Cache Precision (`type_k`/`type_v`), MoE SSD Streaming, Context Shifting, and Speculative Decoding.
- **Dual-Engine Orchestration:** Unifies `llama.cpp` (Reasoning) and `ONNX Runtime` (Vision/Embeddings) into a single shared memory space, eliminating PCIe spill transfer bottlenecks.
- **BitNet & 1.58-bit Ternary Injection:** Dynamically injects `bfe.u32` and PTX instructions at compile time, enabling 4B models to hit extreme speeds on consumer GPUs.

### 2. Agentic Orchestration

- **WASM Skills & Sandboxing:** Secure WebAssembly binaries executing in an isolated memory ring, giving agents native system capabilities without REST API latency. Code executes in a strict WASM sandbox, isolated from the host.
- **Secure MCP & Native Plugin Execution (.dlso):** Manifest-driven execution. The model emits a tool call, and the engine invokes the native plugin's FFI boundary directly, without a localhost network call.
- **Prefix Caching & Delta Decoding:** When a tool returns data or conversations continue, Cluaiz matches the prompt prefix against active KV-cache memory and decodes only the new delta tokens, eliminating redundant prefill computation.
- **Agentic Pause:** If a skill exceeds your VRAM, cluaiz safely offloads calculation to the CPU to prevent OOM errors.
- **Hot-Reload Settings Controller:** Dynamically update YAML manifests (API keys, permissions, model settings) without restarting the engine.

### 3. Generation Control

- **Logit-Level JSON Enforcement:** Force the model to output strictly correct syntax or JSON by applying Logit-level masking during generation.
- **Mid-Generation Pivot (Hot-Steering):** Interrupt long generations with **`Ctrl+C`**. Instead of killing the process, cluaiz **Pauses** the engine. Enter a midway instruction (e.g., "Make it shorter"), and the engine processes this pivot.

### 4. API, Ecosystem & Security

- **API:** Axum + Tokio. Standard OpenAI / Ollama routes (`/v1/chat/completions`, `/api/chat`, `/v1/embeddings`, `/v1/audio/*`, `/v1/models`, `/api/tags`) and CEP (`POST /v1/cep`, `/mcp`) for everything Cluaiz-specific.
- **Native RAG Ingestion (`cluaiz ingest`):** Natively parse, semantically chunk, and save local documents (PDF, TXT, MD) into an LMDB vector storage via ONNX embeddings.
- **Benchmarking (`cluaiz bench`):** Prompt and generation speed (llama-bench).
- **Smart Model Management:** Securely pull, run, and manage GGUF weights directly into the local vault (`~/.cluaiz/models/`).
- **Offline security controls:** `Permission.json` controls telemetry, network access and system isolation.

</details>

---

## 🧭 **Architecture & Under the Hood**

<details open>
<summary><b>Click to expand</b></summary>

### Technical Specification

- **Purpose:** One local engine process that serves chat, embeddings, speech and transcription through
  OpenAI-compatible routes and its own JSON-RPC protocol (CEP), and decides by itself where each model runs
  (GPU, GPU + RAM, CPU) from live hardware measurements.
- **Backends:** `llama.cpp` (cluaiz fork, linked into the engine) for GGUF models; ONNX Runtime for speech and
  vision models.
- **Platforms:** Windows x64 (MSVC), Linux x64 / ARM64 (glibc 2.38+), macOS Apple Silicon.
- **Delivery:** a CPU core plus optional GPU packs (CUDA 12, CUDA 13, Vulkan); the engine loads the GPU backend
  libraries it finds next to its executable at start.

### Architectural Flow

```mermaid
flowchart TB
    subgraph CLIENTS["Clients"]
        CLI["cmd/<br/>CLI and TUI dashboard"]
        APP["Cluaiz App<br/>(desktop GUI)"]
        HTTP["Any OpenAI-compatible client"]
        MCPC["MCP clients"]
        EMB["Embedding apps<br/>(C ABI)"]
    end

    subgraph GATEWAY["Gateway: api/ (Axum HTTP + SSE)"]
        AUTH["Session token auth<br/>host and port from settings"]
        ROUTES["/v1/chat/completions · /v1/embeddings<br/>/v1/audio/speech · /v1/audio/transcriptions<br/>/v1/models · /v1/ingest · /health"]
        CEPR["/v1/cep and /mcp"]
    end

    FFI["ffi/<br/>cep_call · cep_free"]
    CEP["cep/: Cluaiz Engine Protocol<br/>JSON-RPC 2.0 envelope, method registry,<br/>principals, approvals, tasks, discovery"]

    subgraph SERVICES["Request services"]
        SEC["security/<br/>permissions, filesystem jail, audit"]
        CTX["context/<br/>pure context planning: fit, compaction,<br/>memory cards, episodes, recall"]
        AGT["agents/<br/>tools, skills, plugins, MCP client,<br/>tool search, sandboxed execution"]
        MEDIA["engine/media<br/>bounded sources, image / audio / video<br/>decoding, resampling"]
    end

    subgraph RUNTIME["engine/runtime: single engine host"]
        HOST["Model host<br/>load / unload, keep_alive,<br/>one model transition at a time"]
        CHATMEM["Chat memory and KV cache reuse"]
        SPEECH["Speech, transcription and<br/>embedding hosts"]
        VRAMUSE["Measured VRAM use<br/>(learned margins)"]
    end

    subgraph PLANNING["Placement planning"]
        PLAN["engine/plan<br/>candidates: full GPU · GPU + RAM split ·<br/>MoE experts in RAM / SSD · CPU only<br/>context size, KV type, VRAM buffer"]
        HW["engine/hardware<br/>CPU, RAM, GPU (NVML), disk<br/>free memory measured live"]
        MODELS["engine/models<br/>GGUF reader, model registry,<br/>Hugging Face fetcher, capabilities"]
        CORE["engine/core<br/>settings snapshot, paths, units, contracts"]
    end

    subgraph EXEC["Execution"]
        LLAMA["engine/llama<br/>FFI to the llama.cpp fork: chat templates, tokenizer,<br/>prompt chunks, sliding-window checkpoints,<br/>sampling, tool-call parsing, token streaming"]
        ONNX["engine/onnx<br/>ONNX Runtime: Kokoro and other TTS,<br/>Whisper STT, vision encoders"]
    end

    subgraph BACKENDS["Backends loaded at start (core + GPU packs)"]
        GCPU["ggml CPU<br/>(best build for the processor)"]
        GCUDA["ggml CUDA 12 / 13<br/>+ cudart, cuBLAS"]
        GVK["ggml Vulkan"]
        GMTL["ggml Metal<br/>(inside the macOS core)"]
        ORTP["ONNX providers<br/>CUDA (+ cuDNN fetched) · DirectML · CPU"]
    end

    subgraph HARDWARE["Hardware"]
        NV["NVIDIA GPU"]
        AMDI["AMD / Intel GPU"]
        APL["Apple Silicon"]
        CPUH["CPU and RAM"]
        SSD["SSD<br/>(memory-mapped weights)"]
    end

    STORE[("CLUAIZ_HOME<br/>models, settings, session token,<br/>hardware profile, VRAM margins")]

    CLI --> ROUTES
    APP --> ROUTES
    APP --> CEPR
    HTTP --> ROUTES
    MCPC --> CEPR
    EMB --> FFI
    AUTH --- ROUTES
    AUTH --- CEPR
    CEPR --> CEP
    FFI --> CEP
    ROUTES --> SERVICES
    CEP --> SERVICES
    ROUTES --> HOST
    CEP --> HOST
    SERVICES --> HOST
    HOST --> PLAN
    PLAN --> HW
    PLAN --> MODELS
    PLAN --> CORE
    VRAMUSE --> PLAN
    HOST --> CHATMEM
    HOST --> SPEECH
    HOST --> LLAMA
    SPEECH --> ONNX
    LLAMA --> GCPU
    LLAMA --> GCUDA
    LLAMA --> GVK
    LLAMA --> GMTL
    ONNX --> ORTP
    GCUDA --> NV
    ORTP --> NV
    GVK --> AMDI
    GMTL --> APL
    GCPU --> CPUH
    LLAMA -. mmap .-> SSD
    MODELS --- STORE
    HW --- STORE
    VRAMUSE --- STORE
```

### One Chat Request, Step by Step

```mermaid
sequenceDiagram
    autonumber
    participant C as Client
    participant A as api/
    participant X as context/ + agents/
    participant H as engine/runtime
    participant P as engine/plan
    participant L as engine/llama
    participant G as llama.cpp (ggml backend)

    C->>A: POST /v1/chat/completions (stream)
    A->>A: check the session token, validate the request
    A->>X: fit the chat into the context, attach tools and memory
    A->>H: generate(model, messages, settings)
    alt model not loaded, or load settings changed
        H->>P: plan(model geometry, live free RAM and VRAM, settings)
        P-->>H: placement (n_gpu_layers, n_cpu_moe), n_ctx, KV type, VRAM buffer
        H->>L: load the model and context with that plan
        L->>G: llama_model_load, llama_init_from_model, warm-up decode
        H->>H: compare the measured VRAM with the plan, learn the margin
    end
    H->>L: render the chat template, tokenize, reuse the cached prefix
    L->>G: prefill in chunks of 512 (a small tail evened out when weights sit in RAM)
    loop each token
        L->>G: decode one token, sample
        L-->>A: token (answer or reasoning; tool calls parsed)
        A-->>C: SSE chunk
    end
    L-->>A: timings (prompt and generation tokens per second)
    A-->>C: final chunk with usage and engine timings
```

### Workspace Crates

| Crate | Path | Responsibility |
| :--- | :--- | :--- |
| `cmd` | [`cmd/`](cmd/) | The `cluaiz` executable: CLI commands, TUI dashboard, `serve`, `pull`, `run`. |
| `cluaiz-api` | [`api/`](api/) | HTTP and SSE gateway: OpenAI-compatible routes, `/v1/cep`, `/mcp`, auth, boot. |
| `cluaiz-cep` | [`cep/`](cep/) | Cluaiz Engine Protocol: JSON-RPC 2.0 envelope, method registry, approvals, tasks, discovery. |
| `cluaiz-ffi` | [`ffi/`](ffi/) | The engine as a C library: `cep_call` / `cep_free`, the same messages as `POST /v1/cep`. |
| `cluaiz-context` | [`context/`](context/) | Pure context planning (no I/O): fitting, compaction policy, memory cards, episodes, recall. |
| `cluaiz-agents` | [`agents/`](agents/) | Tools, skills, plugins, MCP client, tool search, sandboxed execution. |
| `cluaiz-security` | [`security/`](security/) | Permission schema, filesystem jail, audit log. |
| `cluaiz-runtime` | [`engine/runtime/`](engine/runtime/) | The single model host: load / unload, chat sessions, speech and embedding hosts, VRAM use. |
| `cluaiz-plan` | [`engine/plan/`](engine/plan/) | Placement planner: GPU / split / MoE / CPU candidates, context size, KV type, buffers, learned margins. |
| `cluaiz-hw` | [`engine/hardware/`](engine/hardware/) | Hardware discovery: CPU, RAM, GPU (NVML), disks; free memory measured live. |
| `cluaiz-models` | [`engine/models/`](engine/models/) | GGUF reader, model registry, Hugging Face fetcher, capabilities and taxonomy. |
| `cluaiz-core` | [`engine/core/`](engine/core/) | Settings snapshot, environment paths, units, shared contracts. |
| `cluaiz-media` | [`engine/media/`](engine/media/) | Bounded media sources and decoding: images, audio (resampling), video, documents. |
| `llama-engine` | [`engine/llama/`](engine/llama/) | Safe wrapper over the llama.cpp fork (`engine/llama/llama.cpp`): load, prefill, sampling, streaming, multimodal projection. |
| `onnx-engine` | [`engine/onnx/`](engine/onnx/) | ONNX Runtime models: text-to-speech, Whisper speech-to-text, vision; CUDA / DirectML / CPU providers. |

### Failure & Recovery Logic

| Situation | What the engine does |
| :--- | :--- |
| The model does not fit any placement | The planner refuses before loading and says how much is missing (free memory, lower the context, or a smaller quantization). |
| The planner's estimate is off | llama.cpp's own dry run corrects the estimate once per model; after a load, the measured VRAM is kept as a learned margin for the next plan. |
| llama.cpp cannot allocate the planned GPU layers | The load is retried with fewer GPU layers, then on the CPU if the model fits the free RAM; the response reports the fallback. |
| A load would page the model from the disk without end | A CPU load larger than the free RAM is refused instead of thrashing the SSD (MoE experts are the exception the planner chooses on purpose). |
| The user sets placement or memory values | They are passed to llama.cpp as they are; the planner only reports its estimate. |
| A GPU pack or driver is missing | ggml skips that backend and the model runs on the CPU. |

</details>

---

## 📊 **Hardware, Benchmarks & Troubleshooting**

<details open>
<summary><b>Click to expand</b></summary>

### Hardware Compatibility Matrix

What release v0.1.0 ships (core + GPU packs, chosen by the installer) and where it has run. Details:
[`docs/deployments/platforms.md`](docs/deployments/platforms.md).

| Hardware | Backend | Release package | Tested on |
| :--- | :--- | :--- | :--- |
| **NVIDIA GPU**, Windows | CUDA 12 / CUDA 13 | `win-x64` core + `llama-cuda12` / `llama-cuda13` | ✅ RTX 3050 Laptop 4 GB, Windows 11 (CUDA 13) |
| **NVIDIA GPU**, Linux | CUDA 12 / CUDA 13 | `linux-x64` core + `llama-cuda12` / `llama-cuda13` | ✅ Tesla T4 16 GB, Ubuntu 24.04 (CUDA 12 pack, driver 580) |
| **AMD Radeon / Intel Arc**, Windows and Linux | Vulkan | core + `llama-vulkan` | 🧪 Built, not yet run on this hardware |
| **Apple Silicon** (M1 or newer) | Metal (inside the core) | `mac-arm64` core | 🧪 Built, Mac test pending ([`test/mac`](test/mac)) |
| **x86-64 CPU** (SSE4.2 to AVX-512) | CPU, best build picked at start | any core | ✅ Ryzen 7 7435HS (Windows), Xeon 2.3 GHz (Ubuntu) |
| **ARM64 Linux** | CPU (NEON and newer) | `linux-arm64` core | 🧪 Built, not yet run on hardware |

ROCm/HIP, OpenVINO, SYCL and CANN builds are not part of the release yet.

### Local Hardware Benchmark (Empirical Results: 2026-10-12)

*Empirical regression test data measured on **AMD Ryzen 7 7435HS (8C/16T), 24 GB RAM, NVIDIA GeForce RTX 3050 A Laptop (4 GB VRAM), Windows 11, CUDA 13.2**:*

#### 1. Generation Speed (Tokens / Second)

| Model (Quantization) | Mode | Placement | Cluaiz | Upstream llama.cpp | Delta |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Bonsai 4B (Q1_0)** | auto / gpu | Full GPU | **111.0 t/s** | 106.3 t/s | +4% |
| | split | 18 of 36 blocks on GPU | **27.6 t/s** | 28.2 t/s | -2% |
| **Bonsai 8B** | auto / gpu | Full GPU | **72.7 t/s** | 71.9 t/s | +1% |
| | split | 18 of 36 blocks on GPU | **17.7 t/s** | 18.1 t/s | -2% |
| **Llama 3.2 3B (Q4_K_M)** | auto / gpu | Full GPU | **51.9 t/s** | 51.8 t/s | same |
| | split | 14 of 28 blocks on GPU | **23.4 t/s** | 24.9 t/s | -6% |
| **Qwen3 4B (Q4_K_M)** | auto / gpu | Full GPU | **39.3 t/s** | 38.7 t/s | +2% |
| | split | 18 of 36 blocks on GPU | **15.9 t/s** | 14.9 t/s | +7% |
| **Gemma 4 E4B (Q4_K_M)** | gpu | Full GPU | **32.3 t/s** | 32.0 t/s | +1% |
| | split | 21 of 42 blocks on GPU | **14.5 t/s** | 14.6 t/s | same |
| **Gemma 4 26B MoE (13.3 GB)** | auto (split) | Attention GPU + RAM/SSD Experts | **16.0 - 18.0 t/s** | OOM / ~4.4 t/s | Runs on 4 GB VRAM |

#### 2. Prompt Processing Speed (~530 tokens prompt)

| Model | Full GPU (Cluaiz vs llama.cpp) | Split GPU + RAM (Cluaiz vs llama.cpp) | Split Speedup |
| :--- | :---: | :---: | :---: |
| **Llama 3.2 3B** | 2578 vs 2446 t/s | **1111 vs 810 t/s** | **1.4x faster** |
| **Qwen3 4B** | 1863 vs 1744 t/s | **810 vs 529 t/s** | **1.5x faster** |
| **Gemma 4 E4B** | 1666 vs 1605 t/s | **705 vs 452 t/s** | **1.6x faster** |
| **Bonsai 8B** | 1352 vs 1334 t/s | **769 vs 385 t/s** | **2.0x faster** |
| **Bonsai 4B** | 2270 vs 2129 t/s | **1186 vs 692 t/s** | **1.7x faster** |
| **Bonsai 27B** | 47 vs 30 t/s (shared mem) | **230 vs 110 t/s** | **2.1x faster** |

> [!NOTE]
> **Why Split Prompt is 1.4x–2.2x Faster**: Upstream llama.cpp chunks prompts into fixed 512-token batches (leaving trailing tokens under 32 to run on CPU). Cluaiz balances chunk sizes evenly (e.g. 265 + 265), keeping all prompt chunks on the accelerated path.
> 
> **Time To First Token (TTFT)**: **0.09s - 0.10s** (warm prompt with persistent template caching, down from 0.31s).
> 
> **Complete Reproducible Benchmark**: Full test tables, methodology, and reproduction scripts (`python test/engine_regression/run.py`) are documented in [`test/engine_regression/RESULTS.md`](test/engine_regression/RESULTS.md).
>

### Cloud GPU Benchmark (Empirical Results: 2026-10-28)

*Same test (`test/engine_regression/run.py`) on **Google Cloud `n1-standard-8`: Intel Xeon 2.3 GHz (4C/8T), 29 GB RAM,
NVIDIA Tesla T4 (16 GB VRAM), Ubuntu 24.04, driver 580**. Cluaiz is release v0.1.0 installed with `install.sh`
(CUDA 12 pack); llama.cpp is `llama-bench` built on the same machine from the same llama.cpp commit and given the
placement Cluaiz chose.*

#### 1. Generation Speed (Tokens / Second)

| Model (Quantization) | Mode | Placement | Cluaiz | Upstream llama.cpp | Delta |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Qwen3 4B (Q4_K_M)** | auto / gpu | Full GPU | **65.5 t/s** | 62.3 t/s | +5% |
| | split | 18 of 36 blocks on GPU | **14.5 t/s** | 13.9 t/s | +4% |
| | cpu | CPU only | **7.7 t/s** | 7.9 t/s | same |
| **Gemma 3 12B (Q4_K_M)** | auto / gpu | Full GPU | **17.0 t/s** | 15.7 t/s | +8% |
| | split | 24 of 48 blocks on GPU | **5.5 t/s** | 5.4 t/s | same |
| | cpu | CPU only | **3.1 t/s** | 3.1 t/s | same |
| **Mistral Small 3.2 24B (Q4_K_M, 13.3 GB)** | auto / gpu | Full GPU | **8.0 t/s** | 8.2 t/s | same |
| | split | 20 of 40 blocks on GPU | **3.1 t/s** | 3.1 t/s | same |
| **Gemma 4 31B (Q4_K_M, 17.1 GB)** | auto | 41 of 60 blocks on GPU (planner) | **3.2 t/s** | 3.1 t/s | same |
| | split | 30 of 60 blocks on GPU | **2.3 t/s** | 2.3 t/s | same |
| | cpu | CPU only | **1.3 t/s** | 1.3 t/s | same |
| **Gemma 4 31B (Q8_0, 30.4 GB)** | auto | 23 of 60 blocks on GPU (planner) | **1.3 t/s** | 1.3 t/s | same |

#### 2. Prompt Processing Speed (~535 tokens prompt)

Measured with the prompt fixes on `main` (commits `6120d017`, `4228752a`; not in release v0.1.0). Before them the
T4 showed llama.cpp 7–22% ahead in the cases marked below.

| Model | Full GPU (Cluaiz vs llama.cpp) | Split GPU + RAM (Cluaiz vs llama.cpp) | Split Speedup |
| :--- | :---: | :---: | :---: |
| **Qwen3 4B** | 1935 vs 1978 t/s (v0.1.0: 1758 vs 1882) | **593 vs 393 t/s** | **1.5x faster** |
| **Gemma 3 12B** | 690 vs 725 t/s (v0.1.0: 572 vs 649) | **228 vs 141 t/s** | **1.6x faster** |
| **Mistral Small 3.2 24B** | 372 vs 385 t/s (v0.1.0: 315 vs 340) | 154 vs 153 t/s (1085 tokens) | same |
| **Gemma 4 31B Q4** (planner split, 41/60) | — | **116 vs 117 t/s** (v0.1.0: 96 vs 117) | same |
| **Gemma 4 31B Q8** (planner split, 23/60) | — | **50 vs 51 t/s** (v0.1.0: 45 vs 51) | same |

CPU only, llama.cpp also without a GPU (`-dev none`): Qwen3 4B 27 vs 29, Gemma 3 12B 10 vs 10, Mistral 24B 5 vs 5,
Gemma 4 31B 4 vs 4 t/s.

#### 3. Less RAM and Models Bigger Than RAM + VRAM (same VM with 15 GB RAM)

| Model | What happened | Cluaiz | llama.cpp |
| :--- | :--- | :---: | :---: |
| **Gemma 4 31B Q4** (17.1 GB) | Planner: 41 of 60 blocks on GPU | **2.3 t/s** gen, 112 t/s prompt | 2.3 t/s gen, 112 t/s prompt |
| **Gemma 4 31B Q8** (30.4 GB, dense) | Planner refuses: the closest split is 8 GB short of RAM, with a clear message | not loaded (by design) | — |
| | Forced by the user setting `n_gpu_layers 20`: weights read from the disk on every token | 0.008–0.014 t/s gen, 1.5–3.0 t/s prompt | not measured (test ended) |

> [!NOTE]
> **Reading the T4 numbers**: generation equals llama.cpp in every test (-3% to +8%; the T4's clock moves a few
> percent between runs). Prompt processing equals llama.cpp on full GPU after the two fixes and stays up to 1.6x
> faster when a model is split across GPU and RAM. A dense model larger than RAM + VRAM is refused by the planner
> instead of thrashing the disk; streaming from the SSD is used for MoE experts (Gemma 4 26B above).
>
> **Full Report**: methodology, every setting, the exact llama-bench commands and all logs are in
> [`test/cloud/runs/2026-10-28-run5-linux-t4-regression/RESULTS.md`](test/cloud/runs/2026-10-28-run5-linux-t4-regression/RESULTS.md).
### Hardware & Performance Troubleshooting

cluaiz pushes hardware to its absolute mathematical limits. If you experience unexpected performance drops:

1. **Laptop Power-Saving Throttling:**
   - **Observation:** If your battery drops and is unplugged, Windows forces the GPU into Battery Saver (~10W), dropping TPS to ~15.
   - **Fix:** Plug in your laptop charger. The GPU scales to ~30W+, restoring 30+ TPS.

2. **The "PCIe Spill" Phenomenon:**
   - **Observation:** Allocating too much VRAM forces the system to spill data to Shared GPU Memory (System RAM).
   - **Fix:** cluaiz applies a strict safety margin. Do not override this in `llm_optimization.json` unless you are prepared for massive PCIe latency.

</details>

---

## 🛡️ **Security & Licensing**

<details open>
<summary><b>Click to expand</b></summary>

### Security Architecture

- **Process Isolation**: Kernels execute in restricted sub-processes with strict process-level sandboxing.

- **DNA Verification**: SHA-256 manifest verification for all binary plugins and kernels before dynamic linkage.

### Note on Windows SmartScreen Warning

Since the pre-compiled `cluaiz` executables are built dynamically and are not signed with a commercial certificate, Windows Defender may show a "Windows protected your PC" pop-up.

- **Quick Bypass:** Click "More info", then click "Run anyway".

### License

cluaiz is released under the **Apache License 2.0**. See the [LICENSE](LICENSE) file for more details.

</details>
