# Axum API

The `cluaiz-engine` hosts a high-performance REST API gateway built on the Axum web framework and powered by a multithreaded Tokio runtime. 

---

## Endpoint Specifications

The gateway exposes clean HTTP and Server-Sent Event (SSE) interfaces to interface clients:

### 1. Node Health & Diagnostics
*   **`GET /`**: Server landing route displaying engine version metadata.
*   **`GET /health`**: Diagnostics endpoint returning active check status.
*   **`GET /info`**: Returns host system specifications and CPU core counts.
*   **`GET /status/embedded`**: Confirms host environment parameters.

### 2. Conversational Engine
*   **`POST /chat`**: Asynchronous generation endpoint via Server-Sent Events (SSE). *Now includes Two-Step Discovery: Natively intercepts CEL `<TRIGGER:X>` tokens mid-generation to inject Tool/Extension `SKILL.md` schemas dynamically without crashing RAM.*
*   **`GET /history`**: Lists active chat session IDs and metadata configurations.
*   **`GET /history/{session_id}`**: Retrieves raw, chronological message buffers for a specific session.

### 3. Model Management & Telemetry
*   **`GET /models/available`**: Lists local cached models and available weights in the registry.
*   **`GET /hardware`**: Dynamic readout of GPU/NPU active memory limits and tensor engine loads.
*   **`POST /models/download`**: Spawns a background task to pull and cache weight tensors from remote mirrors.
*   **`POST /models/load`**: Dynamically allocates local RAM/VRAM to mount a specific model into active memory.

### 4. CEL Execution (Direct Engine Control)
*   **`POST /v1/cel/execute`**: Accepts a raw CEL script payload, builds an Execution Plan (AST), and natively executes VRAM/API commands bypassing standard inference workflows.
*   **`POST /v1/execute/:component_name/:function_name`**: Dynamically routes REST calls directly to the specified extension/plugin loaded in the `MasterRegistry`.

### 5. Skills & Extension Hub Management
*   **`GET /v1/skills/list`**: Returns all installed native/WASM skills across domains.
*   **`POST /v1/skills/install`**: Queues the download and WASM compilation of a community extension.
*   **`DELETE /v1/skills/remove`**: Natively unloads and wipes a skill.
*   **`GET /v1/skills/cache` & `DELETE /v1/skills/cache`**: Manages the loaded skill binary cache.
