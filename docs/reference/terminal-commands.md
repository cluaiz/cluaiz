# Cluaiz Core CLI Reference Manual

This is the definitive engineering reference for the `cluaiz` binary. It details the execution flow, internal JSON state mutations, and API mappings of the `cluaiz` commands (`cluaiz --help` is the full list).


---

## 🚀 Core Engine & Orchestration

### `cluaiz` (TUI Dashboard)
* **Usage:** `cluaiz`
* **Description:** Launch the interactive dashboard (TUI).
* **Execution Flow:** Initializes the Ratatui-based Terminal User Interface (TUI). It acts as the primary visual orchestrator, polling the background daemon (if active) or spinning up an embedded local thread. It dynamically polls system health and active engines.
* **API Mapping:** `GET /health` and CEP `system.info` (`POST /v1/cep`).

### `cluaiz serve`
* **Usage:** `cluaiz serve`
* **Action:** Starts the HTTP API on the configured host and port (default port 8000).
* **API Mapping:** The standard routes (`/v1/chat/completions`, `/api/chat`, `/v1/embeddings`, `/v1/audio/*`, `/v1/models`, `/api/tags`, `/api/pull`, `/health`), CEP `POST /v1/cep` and MCP `/mcp`. See [API](../api/README.md).

### `cluaiz mcp-server`
* **Usage:** `cluaiz mcp-server` (launched by an MCP client: `"command": "cluaiz", "args": ["mcp-server"]`)
* **Description:** MCP over stdio, backed by the running Engine's `/mcp`. Read-only methods only.

### `cluaiz ingest`
* **Usage:** `cluaiz ingest <file_path>`
* **Description:** Ingest a document natively for semantic chunking and RAG.
* **Execution Flow:** Invokes the local vectorization pipeline. Parses the document (PDF, TXT, MD), chunks it semantically based on NLP boundaries, and passes the chunks to the active ONNX embedding model to store in the local LMDB namespace.
* **API Mapping:** Maps to `/v1/ingest`.

### `cluaiz setup profile`
* **Usage:** `cluaiz setup profile`
* **Description:** Setup cluaiz Node Profile and Identity (Vectorization and DB Injection).
* **Execution Flow:** Initializes the native Node Profile. Prompts the user for local workspace defaults and configures initial `system_control.json` values for identity and database access.

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
* **API Mapping:** `POST /v1/chat/completions` (OpenAI compatible).

### `cluaiz pull`
* **Usage:** `cluaiz pull <model-id>`
* **Description:** Download and register a model into the local vault without running it.
* **Execution Flow:** Authenticates with HuggingFace Hub or external registry, selects the requested variant, and streams the `.gguf` weights directly to the `~/.cluaiz/models/` secure vault. It exits immediately after download, intentionally preserving VRAM.
* **API Mapping:** CEP `models.download`.

### `cluaiz rm`
* **Usage:** `cluaiz rm <model-id>`
* **Description:** Remove a model from the local vault (physically deletes the .gguf file).
* **Execution Flow:** Verifies if the model is actively loaded in RAM/VRAM. Unlinks metadata and unloads it safely, then physically deletes the `.gguf` file to free up SSD space.
* **API Mapping:** CEP `models.remove`.

### `cluaiz list`
* **Usage:** `cluaiz list`
* **Description:** List all downloaded models in the local vault.
* **Execution Flow:** Scans the `~/.cluaiz/models/` directory and internal manifest files to construct a table showing model IDs, architectures, and physical SSD sizes.
* **API Mapping:** CEP `models.list`; `GET /v1/models` and `GET /api/tags` for OpenAI / Ollama clients.

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
* **API Mapping:** CEP `system.ps`.

### `cluaiz status`
* **Usage:** `cluaiz status`
* **Description:** Show hardware status and silicon health metrics.
* **Execution Flow:** Fetches hardware health, silicone truth metrics (Total VRAM, active layers), and active Booster configurations.
* **API Mapping:** CEP `system.info`.

### `cluaiz calibrate`
* **Usage:** `cluaiz calibrate`
* **Description:** Re-scan hardware and synchronize SiliconTruth profile.
* **Execution Flow:** Runs real-time RDTSC hardware clocking, SIMD profiling, and VRAM boundary detection. Synchronizes this data to create a perfect hardware profile.
* **API Mapping:** none (runs locally).

### `cluaiz bench`
* **Usage:** `cluaiz bench <model>`
* **Description:** Measure prompt and generation speed (llama-bench). Runs locally, without the API.

### `cluaiz config`
* **Usage:** `cluaiz config`
* **Description:** View and change engine settings (`settings.json`).
* **API Mapping:** CEP `config.get` / `config.set` (sections `gguf`, `onnx`, `optimization`, `context`, `storage`, `kv`).

### `cluaiz permission`
* **Usage:** `cluaiz permission <key> <value>`
* **Description:** Change engine permissions and firewalls natively from CLI.
* **Execution Flow:** Edits firewall rules and telemetry flags natively in `Permission.json` (e.g., `wasm_firewall strict`, `stream_telemetry off`). Controls what the active engine is permitted to execute.
* **API Mapping:** CEP `config.get` / `config.set` with `section: "engine"` (permission.json).

### `cluaiz test-prefix`
* **Usage:** `cluaiz test-prefix`
* **Description:** Test Prefix Caching compilation and memory footprint.
* **Execution Flow:** Performs a native dry-run memory allocation to test Prefix Caching, analyzing shared token reuse before actual inference occurs.

---

## 🛠️ Extensibility (Skills, Plugins, MCP)

Cluaiz provides deep native extensibility via Skills, Plugins, and MCP integrations. All extensibility components share a unified command structure supporting aliases like `i` (install), `ls` (list), and `rm` (remove).

### 🧠 Skills
Skills are cognitive instruction sets (`SKILL.md`) that teach the AI how to reason and call tools.
* **`cluaiz skill install <skill_name>`** (alias: `i`)
  * **Flow:** Downloads and installs the skill.
  * **API Mapping:** `tools.install`.
* **`cluaiz skill list`** (alias: `ls`)
  * **Flow:** Lists active installed skills.
  * **API Mapping:** `tools.list`.
* **`cluaiz skill remove <skill_name>`** (alias: `rm`)
  * **Flow:** Deletes the skill and its local footprint.
* **`cluaiz skill cache ls`** & **`cluaiz skill cache clear [cache_id]`**
  * **Flow:** Manages cached skill runtime states.

### ⚙️ Plugins (WASM & Native Execution)
Plugins provide high-performance execution power, either in a secure `WASM` micro-sandbox or as a bare-metal `NATIVE` C-FFI binary (`cluaiz-search`, `cluaiz-db`).
* **`cluaiz plugin install <plugin_name>`** (alias: `i`)
  * **Flow:** Downloads and compiles/installs the plugin.
  * **API Mapping:** `tools.install`.
* **`cluaiz plugin list`** (alias: `ls`)
  * **Flow:** Lists active installed plugins.
  * **API Mapping:** `tools.list`.
* **`cluaiz plugin remove <plugin_name>`** (alias: `rm`)
  * **Flow:** Deletes the plugin and its local binaries.
* **`cluaiz plugin cache ls`** & **`cluaiz plugin cache clear [cache_id]`**
  * **Flow:** Manages cached plugin runtime states.

### 🌐 Model Context Protocol (MCP)
MCP integrations allow Cluaiz to interface with external standard tools.
* **`cluaiz mcp install <mcp_name>`** (alias: `i`)
* **`cluaiz mcp list`** (alias: `ls`)
* **`cluaiz mcp remove <mcp_name>`** (alias: `rm`)
* **`cluaiz mcp start <mcp_name>`**
  * **Flow:** Registers a new MCP server configuration and manages its lifecycle.
* **`cluaiz mcp cache ls`** & **`cluaiz mcp cache clear [cache_id]`**
  * **API Mapping:** `tools.cache` and `tools.clear_cache`.
