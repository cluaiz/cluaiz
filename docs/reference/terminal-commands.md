# Cluaiz Core CLI Reference Manual

This is the definitive engineering reference for the `cluaiz` binary. It details the execution flow, internal JSON state mutations, and API route mappings for all 32 commands natively supported by the Cluaiz engine.


---

## 🚀 Core Engine & Orchestration

### `cluaiz` (TUI Dashboard)
* **Usage:** `cluaiz`
* **Description:** Launch the interactive dashboard (TUI).
* **Execution Flow:** Initializes the Ratatui-based Terminal User Interface (TUI). It acts as the primary visual orchestrator, polling the background daemon (if active) or spinning up an embedded local thread. It dynamically polls system health and active engines.
* **API Mapping:** Internally calls local IPC pipes or polls `GET /health` and `GET /info` to discover active engines.

### `cluaiz serve` (aliases: `api`, `server`)
* **Usage:** `cluaiz serve`
* **Description:** Start the background API Daemon (Server mode).
* **Execution Flow:** Acquires a system-wide lock (`.serve_lock`), initializes the Axum HTTP server on port 8000, and spawns the Named Pipe IPC listener (`\\.\pipe\cluaiz_engine_pipe`). This daemon exposes the Cluaiz engine to local applications.
* **API Mapping:** Binds all 36+ REST endpoints defined in `routes.rs` and initializes the HTTP listener.

### `cluaiz ingest`
* **Usage:** `cluaiz ingest <file_path>`
* **Description:** Ingest a document natively for semantic chunking and RAG.
* **Execution Flow:** Invokes the local vectorization pipeline. Parses the document (PDF, TXT, MD), chunks it semantically based on NLP boundaries, and passes the chunks to the active ONNX embedding model to store in the local LMDB namespace.
* **API Mapping:** Maps to `POST /v1/ingest/file` (`ingest::file_ingest`).

### `cluaiz setup profile`
* **Usage:** `cluaiz setup profile`
* **Description:** Setup cluaiz Node Profile and Identity (Vectorization and DB Injection).
* **Execution Flow:** Initializes the native Node Profile. Prompts the user for local workspace defaults and configures initial `system_control.json` values for identity and database access.

### `cluaiz cel execute`
* **Usage:** `cluaiz cel execute <script.cel>`
* **Description:** Natively executes a cluaiz Execution Language (CEL) script.
* **Execution Flow:** The Engine parses the CEL script's Abstract Syntax Tree (AST) and hooks directly into native C-Pointers in shared memory (`payload_ptr`) to execute complex agentic logic mid-inference without requiring an external Python SDK.
* **API Mapping:** `POST /v1/cel/execute` (`cel_handler::execute_cel_script`).

### `cluaiz cel test`
* **Usage:** `cluaiz cel test <script.cel>`
* **Description:** Compiles and validates a CEL script without executing state-mutating actions.
* **Execution Flow:** Runs the CEL compiler pipeline to validate syntax and sandbox constraints (dry-run mode).

### `cluaiz help`
* **Usage:** `cluaiz help`
* **Description:** Show this help screen.
* **Execution Flow:** Parses `cmd/assets/commands.json` dynamically and generates the CLI `--help` stdout screen based on the JSON configuration.

---

## 🧠 Model Management & Inference

> [!IMPORTANT]
> **Understanding `run` vs `pull` vs `rm`**
> - `run` will **automatically download** the model if it is missing, and then immediately load it into VRAM for inference.
> - `pull` will **only download** the model to your SSD. It is meant for background downloading and will NOT load the model into VRAM.
> - `rm` physically **deletes** the downloaded weights from your SSD to free up space.

### `cluaiz run`
* **Usage:** `cluaiz run <model-id>`
* **Description:** Pull & run a model. Downloads if not cached locally.
* **Execution Flow:** 
  1. Checks if the model is a HuggingFace repo. If so, fetches metadata and prompts for GGUF variant selection.
  2. If the model is not on disk, it automatically pulls it.
  3. Conducts a Pre-flight Silicon Audit (checks VRAM, RAM, and projects TPS).
  4. Spawns the native inference thread via FFI and launches the Ratatui Dashboard for interactive chat.
* **API Mapping:** `POST /v1/chat/completions` (OpenAI compatible) and `POST /chat` (legacy).

### `cluaiz pull`
* **Usage:** `cluaiz pull <model-id>`
* **Description:** Download and register a model into the local vault without running it.
* **Execution Flow:** Authenticates with HuggingFace Hub or external registry, selects the requested variant, and streams the `.gguf` weights directly to the `~/.cluaiz/models/` secure vault. It exits immediately after download, intentionally preserving VRAM.
* **API Mapping:** `POST /api/pull` (`models::pull_model`) & `POST /models/download`.

### `cluaiz rm`
* **Usage:** `cluaiz rm <model-id>`
* **Description:** Remove a model from the local vault (physically deletes the .gguf file).
* **Execution Flow:** Verifies if the model is actively loaded in RAM/VRAM. Unlinks metadata and unloads it safely, then physically deletes the `.gguf` file to free up SSD space.
* **API Mapping:** `DELETE /v1/models/{model_id}` (`models::rm_model`).

### `cluaiz list`
* **Usage:** `cluaiz list`
* **Description:** List all downloaded models in the local vault.
* **Execution Flow:** Scans the `~/.cluaiz/models/` directory and internal manifest files to construct a table showing model IDs, architectures, and physical SSD sizes.
* **API Mapping:** `GET /v1/models/installed` (`models::list_installed_models`) & `GET /api/tags`.

### `cluaiz model`
* **Usage:** `cluaiz model <set-chat|set-vector> <model_id>`
* **Description:** Switch the active chat or vector/embedding model dynamically.
* **Execution Flow:** Modifies the active routing defaults in `Permission.json`. Directs all subsequent implicit chat or vector embedding payload traffic to the newly selected backend model without requiring a daemon restart.

---

## ⚙️ System, Hardware & Tuning

### `cluaiz ps`
* **Usage:** `cluaiz ps`
* **Description:** Show active neural engines in memory and auto-heal stale PIDs.
* **Execution Flow:** Probes active Neural Engines and memory-mapped files. Detects if a daemon crashed and auto-heals stale Process IDs.
* **API Mapping:** `GET /v1/system/ps` (`ps::get_processes`).

### `cluaiz status`
* **Usage:** `cluaiz status`
* **Description:** Show hardware status and silicon health metrics.
* **Execution Flow:** Fetches hardware health, silicone truth metrics (Total VRAM, active layers), and active Booster configurations.
* **API Mapping:** Iterates `GET /v1/booster/status` and `GET /info`.

### `cluaiz calibrate`
* **Usage:** `cluaiz calibrate`
* **Description:** Re-scan hardware and synchronize SiliconTruth profile.
* **Execution Flow:** Runs real-time RDTSC hardware clocking, SIMD profiling, and VRAM boundary detection. Synchronizes this data to create a perfect hardware profile.
* **API Mapping:** `POST /v1/hardware/calibrate` (`models::calibrate`).

### `cluaiz benchmark`
* **Usage:** `cluaiz benchmark`
* **Description:** Run a full hardware performance benchmark.
* **Execution Flow:** Stress tests CPU/GPU via standardized neural prompt sequences. Outputs Tokens/Sec (TPS), Time To First Token (TTFT), and wattage reports to the `test/benchmark/` directory.
* **API Mapping:** `POST /v1/benchmark/run` (`benchmark::run`).

### `cluaiz booster`
* **Usage:** `cluaiz booster` (Can accept flags like `--kv-quant`)
* **Description:** View or configure the system performance booster settings interactively.
* **Execution Flow:** Interactively configures or injects parameters into `system_booster.json` (e.g. Context Shifting limits, FlashAttention modes, KV Cache quantization). The memory arbiter applies these natively to optimize inference.
* **API Mapping:** `GET /v1/booster/status` and `POST /v1/booster/update` (`booster::update`).

### `cluaiz permission`
* **Usage:** `cluaiz permission <key> <value>`
* **Description:** Change engine permissions and firewalls natively from CLI.
* **Execution Flow:** Edits firewall rules and telemetry flags natively in `Permission.json` (e.g., `wasm_firewall strict`, `stream_telemetry off`). Controls what the active engine is permitted to execute.
* **API Mapping:** `GET /v1/system/permission` and `POST /v1/system/permission` (`permission::update_permission`).

### `cluaiz test-jit`
* **Usage:** `cluaiz test-jit`
* **Description:** Test JIT KV Cache compilation and memory footprint.
* **Execution Flow:** Performs a native dry-run memory allocation to test Just-In-Time KV Cache compilation, analyzing exactly how much VRAM a prompt will consume before actual inference occurs.

---

## 🛠️ Extensibility (Skills, Plugins, MCP, Extensions)

Cluaiz provides deep native extensibility via WASM Skills, Native Plugins, and MCP integrations. All extensibility components share a unified command structure supporting aliases like `i` (install), `ls` (list), and `rm` (remove).

### 🧠 WASM Skills
Skills are isolated WebAssembly binaries that execute securely within the Cluaiz sandbox.
* **`cluaiz skill install <skill_name>`** (alias: `i`)
  * **Flow:** Downloads and compiles the WASM binary. 
  * **API Mapping:** `POST /v1/skills/install` (`skills::install_skill`).
* **`cluaiz skill list`** (alias: `ls`)
  * **Flow:** Lists active WASM sandboxes. 
  * **API Mapping:** `GET /v1/skills/list` (`skills::list_skills`).
* **`cluaiz skill remove <skill_name>`** (alias: `rm`)
  * **Flow:** Deletes the skill and its local footprint.
* **`cluaiz skill cache ls`**
  * **Flow:** Lists dual-caches (pre-computed KV cache states for fast skill routing). 
  * **API Mapping:** `GET /v1/skills/cache`.
* **`cluaiz skill cache clear [cache_id]`**
  * **Flow:** Wipes orphaned KV states to free SSD space. 
  * **API Mapping:** `DELETE /v1/skills/cache`.

### ⚙️ Native Plugins
Plugins are OS-native C/Rust FFI binaries (Muscles) that provide deep OS access.
* **`cluaiz plugin install <plugin_name>`** (alias: `i`)
* **`cluaiz plugin list`** (alias: `ls`)
* **`cluaiz plugin remove <plugin_name>`** (alias: `rm`)
* **`cluaiz plugin link <plugin_name> <skill_name>`**
  * **Flow:** Links a native plugin binary to a specific WASM skill sandbox.
* **`cluaiz plugin cache ls`** & **`cluaiz plugin cache clear [cache_id]`**
  * **Flow:** Manages cached plugin runtime states. 

### 📦 Extensions
Extensions are complex bundles combining Brain (WASM Skills) and Muscles (Native Plugins).
* **Command Alias:** `cluaiz ext` can be used instead of `cluaiz extension`.
* **`cluaiz extension install <extension_name>`** (alias: `i`)
* **`cluaiz extension list`** (alias: `ls`)
* **`cluaiz extension remove <extension_name>`** (alias: `rm`)
* **`cluaiz extension start <extension_name>`**
  * **Flow:** Starts background daemons for Extension bundles.
* **`cluaiz extension cache ls`** & **`cluaiz extension cache clear [cache_id]`**

### 🌐 Model Context Protocol (MCP)
MCP integrations allow Cluaiz to interface with external standard tools.
* **`cluaiz mcp install <mcp_name>`** (alias: `i`)
* **`cluaiz mcp list`** (alias: `ls`)
* **`cluaiz mcp remove <mcp_name>`** (alias: `rm`)
* **`cluaiz mcp start <mcp_name>`**
  * **Flow:** Registers a new MCP server configuration and manages its lifecycle.
* **`cluaiz mcp cache ls`** & **`cluaiz mcp cache clear [cache_id]`**
  * **API Mapping:** `GET /v1/mcp/cache` and `DELETE /v1/mcp/cache`.
