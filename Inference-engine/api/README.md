# 🌐 Cluaize API Architecture & Guidelines (`inference-engine/api`)

This document establishes the strict architectural rules, API design paradigms, and folder structures for the **Cluaize Inference Engine API**. 

## 🏗️ 1. Dual-Gateway Architecture

The Cluaize API is designed to serve two entirely different paradigms simultaneously without coupling them:

1. **Native FFI / IPC Gateway (0.00ms Latency)**
   - **Target:** Cluaize Native Desktop App, Native CLI.
   - **Protocol:** Named Pipes / Shared Memory.
   - **Rule:** Native clients MUST NOT use the HTTP API. They must connect directly to the IPC Daemon pipe to stream tokens and send CDQL (Cluaize Data Query Language) commands.

2. **HTTP REST Gateway (Port 8000)**
   - **Target:** Web Apps, Mobile Apps, Raspberry Pi, External Cluster Servers, Third-Party Developers.
   - **Protocol:** HTTP/1.1 and Server-Sent Events (SSE) for streaming.
   - **Rule:** Strict validation. Operates completely independently from the Native FFI pipe.

3. **Pure CEL Gateway (Hardcore Engine Control)**
   - **Target:** Internal AI feedback loops, Advanced DevOps, Docker Nodes.
   - **Protocol:** Raw CEL Script over POST (`/v1/cel/execute`) or Mid-Inference AI Hooks (`<cel>...`).
   - **Rule:** Direct interaction with `neural_foundry`. No intermediate plugins required for engine-level hooks like `kv_cache -> clear()` or `mid_layer -> inject()`.

---

## 📏 2. Folder Structure Guidelines

To maintain industrial-grade code quality, the `api` directory must strictly follow this structure:

```text
api/
├── src/
│   ├── main.rs          # Bootstrapper, Server Initialization (Axum & IPC daemon spawn)
│   ├── routes.rs        # All HTTP route registrations and CORS policies
│   ├── ffi_bridge.rs    # [NEW] Native Named Pipe / IPC Listener for Desktop/CLI
│   ├── handlers/        # Business logic for endpoints
│   │   ├── chat.rs      # Inference & Token Streaming logic (SSE)
│   │   ├── models.rs    # Model downloading, loading, and hardware probing
│   │   ├── system.rs    # Health checks, `skip_think` interrupts, hardware telemetry
│   │   ├── cel_handler.rs # [NEW] Pure CEL Execution API (`/v1/cel/execute`)
│   │   └── history.rs   # [NEW] EmbeddedManager integration for DB fetch (CDQL HTTP wrapper)
│   └── models/          # Request/Response JSON Data Structures (Strict Typing)
```

---

## ⚙️ 3. API Design Philosophy (Inspired by Market Leaders)

Based on extensive research of the current LLM serving market,  adopts a **Hybrid Design**:

### The vLLM Approach (Standardization & Throughput)
- Like vLLM, 's HTTP API will offer **OpenAI-Compatible Endpoints** (e.g., `/v1/chat/completions`) for the external REST API. This allows developers to use existing OpenAI libraries instantly.
- **Streaming:** Implement Server-Sent Events (SSE) standard for streaming responses.

### Local Lifecycle Management
-  will feature custom, intuitive endpoints for local model management, heavily mapped to our custom Silicon probing (`HardwareGovernor`).
- **Endpoints:** `/api/models/pull`, `/api/models/list`.

---

## 🚫 4. Strict "DO NOT" Rules (Kayde Kanoon)
1. **NO Python / NO Docker:** The API must compile down to a single bare-metal Rust executable.
2. **NO Local Database Mocking in UI:** The API must serve data directly from `~/.cluaize/cluaizd` LMDB.
3. **NO Blocking Operations in Chat:** Inference endpoints must immediately yield to Tokio's async runtime to prevent locking up the server.
