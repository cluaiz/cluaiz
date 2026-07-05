# Cluaiz Unified API & FFI Reference Manual

This document is the unified, comprehensive reference manual for the Cluaiz Axum API Gateway and the FFI C-ABI interface. It covers all exposed REST endpoints, JSON payload structures, SSE formats, and FFI C-pointer bindings used for zero-copy computation.


---

## 🏛️ Foundational Protocols: REST vs. FFI

The Cluaiz engine utilizes two distinct communication planes depending on developer requirements:
1. **REST Gateway (HTTP/SSE):** Operates on `http://127.0.0.1:8000` (localhost restricted for safety). Designed for client interfaces, tools, and remote workspaces.

2. **IPC Named Pipe & C-ABI FFI:** Operates via Windows Named Pipes (`\\.\pipe\cluaiz_engine_pipe`) and dynamic pointer bindings. Designed for local zero-latency workflows. Detailed in the [Zero-Latency Architecture Overview](../architecture/unified-brain-ffi.md).

For a deeper dive into how this relates to model loading and multimodal routing, see the [Dual-Engine Architecture Guide](../engine/dual_engine_architecture.md).




---

## 📡 1. Core System & Control Endpoints

### `GET /health`
*   **Description:** Performs a diagnostic scan of engine uptime and base daemon responsiveness.
 
*   **Request Format:** `GET /health`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "alive",
      "engine": "Cluaiz Inference Engine",
      "version": "0.1.0",
      "message": "🚀 Cluaiz is alive! All systems operational."
    }
    ```
 
 
### `GET /info`
*   **Description:** Returns core system pillars, backend dependency maps, and operational directives.
*   **Request Format:** `GET /info`
*   **Response Schema (200 OK):**
    ```json
    {
      "engine": "Cluaiz",
      "full_name": "Cluaiz Inference Engine",
      "version": "0.1.0",
      "pillars": {
        "api": "Gateway — HTTP server on port 8000 (this!)",
        "kernel": "Brain — Decision-making & orchestration",
        "storage": "Sidecars — 5 Official DB engines (Mongo, Neo4j, ClickHouse, Qdrant, MinIO)",
        "engines": "Muscles — C++ model inference via llama.cpp FFI"
      },
      "philosophy": "Nothing Need. Just Cluaiz.",
      "banned": ["Python", "Docker", "npm", "pip"]
    }
    ```



### `GET /v1/system/ps`
*   **Description:** Scans active model orchestration tasks loaded in memory.
*   **Request Format:** `GET /v1/system/ps`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "active_processes": [
        {
          "pid": "29514",
          "model_id": "bonsai:8b",
          "vram_gb": 4.2,
          "context_size": 8192,
          "engine": "llama.cpp"
        }
      ]
    }
    ```

### `GET /v1/system/control`
*   **Description:** Retrieves host environment profiles and runtime system control parameters from `system_control.json`. For details on the hardware governance mapping layer, see the [System Control Deep Dive](../engine/system_control.md).
*   **Request Format:** `GET /v1/system/control`

*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "control": {
        "node_id": "cluaiz-node-x1",
        "active_model": "bonsai:8b",
        "user_identity": {
          "name": "Operator",
          "purpose": "PRODUCTION"
        },
        "hardware_governance": {
          "vram_limit_gb": 12.0,
          "cpu_thread_limit": 8,
          "allow_speculative_decoding": true,
          "fallback_to_cpu": true
        },
        "network": {
          "api_host": "127.0.0.1",
          "api_port": 3000,
          "enable_cors": true
        }
      }
    }
    ```

### `GET /v1/system/permission`
*   **Description:** Reads permissions and sandbox environment profiles defined in `Permission.json`. For structural details, see the [Permissions Architecture Manual](../engine/permission.md).


*   **Request Format:** `GET /v1/system/permission`
*   **Response Schema (200 OK):**
    ```json
    {
      "firewall_mode": "strict",
      "enable_telemetry": false,
      "vectorize_user_input": true,
      "vectorize_ai_response": true,
      "chat_ttl_hours": 24,
      "default_chat_model": "bonsai:8b",
      "default_vector_model": "bge_m3:unknown:onnx:fp32"
    }
    ```

### `POST /v1/system/permission`
*   **Description:** Modifies a single permission configuration key dynamically.
*   **Request Format:** `POST /v1/system/permission`
*   **Request Payload:**
    ```json
    {
      "key": "firewall_mode",
      "value": "relaxed"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success"
    }
    ```

### `POST /engine/skip_think`
*   **Description:** Injects a global signal directing the neural engine to skip `<think>` reasoning block parsing.
*   **Request Format:** `POST /engine/skip_think`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "message": "Brain skip signal injected. Neural graph will pivot."
    }
    ```

### `POST /v1/system/cmd`
*   **Description:** Executes raw shell commands locally. Restricts access to localhost (`127.0.0.1`) only.
*   **Request Format:** `POST /v1/system/cmd`
*   **Request Payload:**
    ```json
    {
      "command": "dir"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "output": "Directory listing output...\n"
    }
    ```
*   **Error Response (403 Forbidden):** Returns immediately if accessed from a non-loopback remote interface.
    ```json
    {
      "status": "error",
      "output": "Access Denied: 403 Forbidden. Execution strictly restricted to localhost (127.0.0.1)."
    }
    ```

---

## 🧠 2. Inference & Model Management

### `POST /v1/chat/completions` (OpenAI Compatible)
*   **Description:** Main streaming inference endpoint supporting Server-Sent Events (SSE). 
*   **Request Format:** `POST /v1/chat/completions`
*   **Request Payload:**
    ```json
    {
      "model": "bonsai:8b",
      "messages": [
        { "role": "user", "content": "Explain C-ABI in 1 sentence." }
      ],
      "stream": true,
      "temperature": 0.3
    }
    ```
*   **Response Schema (SSE stream format):**
    ```text
    data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":"C-ABI"},"finish_reason":null}]}
    data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"delta":{"content":" defines"},"finish_reason":null}]}
    data: [DONE]
    ```

### `POST /v1/chat/stream` (Simple Stream)
*   **Description:** Yields raw text tokens directly as they compile.
*   **Request Format:** `POST /v1/chat/stream`
*   **Request Payload:**
    ```json
    {
      "message": "Hello"
    }
    ```
*   **Response Format (`text/event-stream`):**
    ```text
    data: Hello
    data:  there!
    ```

### `GET /hardware`
*   **Description:** Queries system hardware specs, total VRAM, and SIMD optimization status.
*   **Request Format:** `GET /hardware`
*   **Response Schema (200 OK):**
    ```json
    {
      "cpu": "Intel Core i7-12700H",
      "ram_total_gb": 16.0,
      "ram_free_gb": 4.2,
      "accelerator": "CUDA (NVIDIA GeForce RTX 3050 Laptop GPU)",
      "vram_total_gb": 4.0,
      "vram_free_gb": 1.1
    }
    ```

### `GET /v1/models/installed`
*   **Description:** Lists GGUF neural weights currently registered inside the local secure vault.
*   **Request Format:** `GET /v1/models/installed`
*   **Response Schema (200 OK):**
    ```json
    [
      {
        "model_id": "bonsai:8b",
        "type": "chat",
        "size_bytes": 4820000000,
        "path": "C:\\Users\\Aryan\\.cluaiz\\models\\chat\\bonsai-8b.gguf"
      }
    ]
    ```

### `GET /api/tags` (or `/models/available`)
*   **Description:** Queries the centralized model registry index to fetch compatible variant tags.
*   **Request Format:** `GET /api/tags`
*   **Response Schema (200 OK):**
    ```json
    {
      "models": [
        {
          "name": "bonsai:8b",
          "architecture": "Llama",
          "quant": "q4_k_m"
        }
      ]
    }
    ```

### `POST /models/download` (or `/api/pull`)
*   **Description:** Launches a background download task fetching GGUF weights to SSD.
*   **Request Format:** `POST /models/download`
*   **Request Payload:**
    ```json
    {
      "model_id": "bonsai:8b"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "task_id": "dl_bonsai_8b_f7g8",
      "status": "downloading",
      "bytes_total": 4820000000
    }
    ```

### `POST /models/load`
*   **Description:** Mounts a selected model dynamically from SSD vault to active RAM/VRAM.
*   **Request Format:** `POST /models/load`
*   **Request Payload:**
    ```json
    {
      "model_id": "bonsai:8b"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "loaded",
      "model_id": "bonsai:8b",
      "time_elapsed_ms": 1420
    }
    ```
*   **Error Response (500 Internal Server Error - VRAM Allocation Failure):**
    ```json
    {
      "status": "error",
      "message": "CUDA OOM: Insufficient VRAM to allocate model layers."
    }
    ```

### `DELETE /v1/models/{model_id}`
*   **Description:** Safely unloads a model and deletes it from local disk space.
*   **Request Format:** `DELETE /v1/models/bonsai:8b`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "message": "Model removed"
    }
    ```

---

## ⚙️ 3. Hardware Tuning & Booster Configuration

### `GET /v1/booster/status`
*   **Description:** Returns dynamic speed tuning settings loaded in `system_booster.json`. For details on the optimization profiles, see the [System Booster Deep Guide](../engine/booster.md).
*   **Request Format:** `GET /v1/booster/status`
*   **Response Schema (200 OK):**
    ```json
    {
      "booster": {
        "auto_round": "Auto",
        "context_shifting": "Auto",
        "dflash": "Auto",
        "enforce_json": false,
        "flash_attention": "On",
        "force_memory_lock": "Off",
        "force_vram_reclaim": "Off",
        "kv_cache_quantization": "Auto",
        "mode_run": "multitasking",
        "n_gpu_layers": -1,
        "response_length": "auto",
        "speculative_decoding": "Off",
        "think_mode": "Off",
        "turbo_quant": "On"
      },
      "status": "success"
    }
    ```

### `POST /v1/booster/update`
*   **Description:** Updates a specific booster parameter instantly in active hardware governor.
*   **Request Format:** `POST /v1/booster/update`
*   **Request Payload:**
    ```json
    {
      "key": "flash_attention",
      "value": "On"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success"
    }
    ```

### `POST /v1/benchmark/run`
*   **Description:** Initiates standard prompt execution test arrays to grade workstation FLOPs/TPS performance.
*   **Request Format:** `POST /v1/benchmark/run`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "scores": {
        "cpu_flops": 2450.2,
        "gpu_flops": 12050.5,
        "ram_bandwidth_gbps": 45.2
      }
    }
    ```

---

## 🔌 4. Extensibility: Skills, Plugins & MCP

### `GET /v1/skills/list`
*   **Description:** Lists the active WASM scripts running inside the engine's sandbox.
*   **Request Format:** `GET /v1/skills/list`
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "skills": ["web_search", "data_formatter"]
    }
    ```

### `POST /v1/skills/install`
*   **Description:** Installs a new sandboxed WebAssembly skill.
*   **Request Format:** `POST /v1/skills/install`
*   **Request Payload:**
    ```json
    {
      "skill_name": "web_search",
      "source_url": "https://cluaiz.com/skills/web_search.wasm"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "message": "WASM Skill installed"
    }
    ```

### `DELETE /v1/skills/remove`
*   **Description:** Permanently deletes a WASM skill.
*   **Request Format:** `DELETE /v1/skills/remove`
*   **Request Payload:**
    ```json
    {
      "skill_name": "web_search"
    }
    ```
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success"
    }
    ```

### `GET /v1/skills/cache` & `DELETE /v1/skills/cache`
*   **Description:** Fetches or clears pre-computed KV-caches associated with sandboxed agents.
*   **Response Schema (200 OK):**
    ```json
    {
      "status": "success",
      "cleared_skills": 2
    }
    ```

---

## 🧱 5. C-ABI FFI IPC Specification (Zero-Copy Bridge)

For zero-copy data pipelines, client wrappers bind directly to the engine's memory space. For details on how mid-layer FFI interventions pause generation loops to inject context, see the [JIT Injection Architecture Explanation](../engine/jit_architecture.md).



### Memory Representation (C-ABI Representation)
FFI payload structs are marked `#[repr(C)]` to guarantee stable alignment across languages:

```rust
#[repr(C)]
pub enum PayloadType {
    Bincode = 0,
    RawJson = 1,
}

#[repr(C)]
pub struct ExtensionPayload {
    pub payload_type: PayloadType,
    pub data_ptr: *const u8,
    pub data_len: usize,
}
```

### Windows IPC Named Pipe Packet Structure
Clients communicate through `\\.\pipe\cluaiz_engine_pipe` using JSON payload wrapping. 

To execute an extension directly over memory-mapped pointers, the client writes a packet with action `EXTENSION_PAYLOAD`:

```json
{
  "action": "EXTENSION_PAYLOAD",
  "extension_name": "vector_accelerator",
  "payload": "{\"query\":\"tensor_search\",\"values\":[0.1,0.5,0.9]}"
}
```

The engine resolves dynamic function symbols via POSIX `dlopen` / Windows `LoadLibraryW` and writes back output:
```json
{
  "status": "success",
  "results": [42, 109, 8]
}
```



---

## ⚙️ Universal Component Settings Controller

### `POST /api/components/settings`

Dynamically hot-reloads and updates any component's manifest (`manifest-extension.yaml`, `manifest-mcp.yaml`, `manifest-plugin.yaml`, `SKILL.md`) at runtime, without an engine restart. This is universally applicable to all plugins, extensions, MCPs, and skills.

The engine uses a schema-less `serde_yaml::Value` deep-merge strategy. This means you can inject or modify any arbitrary nested keys inside any section (`settings`, `permissions`, `discovery`, `activation`) and it will perfectly preserve the rest of the file.

#### Example 1: Updating an Extension's API Keys
Used to switch API providers or keys dynamically (e.g. `cluaiz-search` from SerpAPI to Tavily).
**Request Payload:**
```json
{
  "component_type": "extension",
  "component_id": "cluaiz-search",
  "updates": {
    "settings": {
      "search_api_type": "tavily",
      "search_api_key": "tvly-xxxxxxxxxxxx"
    }
  }
}
```

#### Example 2: Updating an MCP's Permissions
Used to grant or revoke network access or file system constraints dynamically for an MCP.
**Request Payload:**
```json
{
  "component_type": "mcp",
  "component_id": "postgres-connector",
  "updates": {
    "permissions": {
      "network_access": true,
      "allowed_domains": ["db.internal.com"],
      "allow_subprocess": false
    }
  }
}
```

#### Example 3: Updating a Skill's Parameters
Updates the YAML frontmatter inside a `SKILL.md` file without touching the Markdown instructions body.
**Request Payload:**
```json
{
  "component_type": "skill",
  "component_id": "coding-assistant",
  "updates": {
    "settings": {
      "temperature": 0.2,
      "max_tokens": 8192
    }
  }
}
```

**Response (200 OK)**
```json
{"status":"success"}
```
