# 🧬 cluaiz CLI — Complete Command Reference (A to Z)

> Binary: `cluaiz` — Sovereign Neural Kernel  
> Built from: `cluaiz/cmd/`

---

## 🔨 BUILD & DEV COMMANDS

> These are NOT `cluaiz` commands. These run inside the workspace during development.

| Command | Description |
|---------|-------------|
| `cargo build --release --workspace` | Full workspace release build (all crates) |
| `cargo build --release --package cluaiz_api` | Only the engine API/IPC daemon binary |
| `cargo build --release --package cmd` | Only the CLI binary |
| `cargo run -- serve` | Dev-run the engine daemon (no install needed) |
| `cargo run -- run <model-id>` | Dev-run the CLI |
| `Copy-Item "target\release\cluaiz.exe" "$env:USERPROFILE\.cluaiz\bin\cluaiz.exe" -Force` | Install built binary to system path |

---

## 🚀 CORE COMMANDS

| Command | Description |
|---------|-------------|
| `cluaiz` | Launches the interactive Main Menu TUI |
| `cluaiz menu` | Explicitly open the Main Menu TUI |
| `cluaiz help` | Show the rich formatted help screen (loads commands.json) |
| `cluaiz serve` | Start the background Engine API Daemon on `http://localhost:8000` + Named Pipe IPC |

---

## 🤖 MODEL COMMANDS

| Command | Flags / Args | Description |
|---------|-------------|-------------|
| `cluaiz run` | _(no args)_ | Opens the Dashboard TUI |
| `cluaiz run <model-id>` | `--interactive true/false` | Pull + execute a model. Downloads if not cached. |
| `cluaiz run <model-id> --interactive false` | | Run in non-interactive single-pass mode |
| `cluaiz pull <model-id>` | | Download and register a model into local vault |
| `cluaiz list` | | List all downloaded models in the vault |
| `cluaiz rm <model-id>` | | Remove a model from the local vault |
| `cluaiz model set-chat <model-id>` | | Set the active chat/LLM model in Permission.json |
| `cluaiz model set-vector <model-id>` | | Set the active vector/embedding model in Permission.json |

**Examples:**
```bash
cluaiz run gemma4:e2b
cluaiz run bonsai:8b --interactive false
cluaiz pull qwen3:8b
cluaiz pull unsloth/Qwen3.5-4B-GGUF
cluaiz rm gemma4:e2b
cluaiz model set-chat gemma4:e2b
cluaiz model set-vector bge_m3:unknown:onnx:fp32
```

---

## ⚙️ SYSTEM COMMANDS

| Command | Flags / Args | Description |
|---------|-------------|-------------|
| `cluaiz status` | | Show hardware health, silicon profile, active drivers |
| `cluaiz calibrate` | | Re-scan hardware and synchronize SiliconTruth profile |
| `cluaiz --calibrate` | _(legacy flag)_ | Same as `calibrate` (older style) |
| `cluaiz benchmark` | | Run full hardware performance benchmark |
| `cluaiz benchmark <model-id>` | `--runs <N>` | Benchmark a specific model N times |
| `cluaiz --benchmark` | _(legacy flag)_ | Same as `benchmark` (older style) |
| `cluaiz logs stream --tail` | | Stream active logs to terminal |
| `cluaiz ps` | | Show active neural engines loaded in memory |
| `cluaiz test-jit` | | Test JIT KV Cache compilation and memory footprint |

**Examples:**
```bash
cluaiz status
cluaiz calibrate
cluaiz benchmark
cluaiz benchmark gemma4:e2b --runs 3
cluaiz ps
```

---

## 🧠 BRAIN / FFI DATABASE COMMANDS

> Controls the FFI connection to the `cluaizdb` background database daemon.

| Command | Args | Description |
|---------|------|-------------|
| `cluaiz brain on` | _(no args = local)_ | Enable FFI Database connection (local cluaizdb) |
| `cluaiz brain on <ip:port>` | e.g. `10.0.0.5:8080` | Connect to a remote cluaizdb instance |
| `cluaiz brain off` | | Disable FFI Database connection |
| `cluaiz brain only` | | Pure Brain Mode: enable local DB but suspend LLM to save VRAM |
| `cluaiz brain status` | | View connection status and background daemon health |

**Examples:**
```bash
cluaiz brain on
cluaiz brain on 10.0.0.5:8080
cluaiz brain off
cluaiz brain only
cluaiz brain status
```

---

## 🔐 PERMISSION COMMANDS

> Controls `Permission.json` — security, privacy, vectorization settings.

| Command | Args | Description |
|---------|------|-------------|
| `cluaiz permission` | _(no args)_ | Open interactive permission TUI menu |
| `cluaiz permission firewall <status>` | `auto / strict / off` | Set WASM Firewall mode |
| `cluaiz permission telemetry <status>` | `on / off` | Enable or disable anonymous telemetry |

**Interactive Menu Options (when run without args):**
- WASM Firewall → `auto / strict / off`
- Telemetry → `true / false`
- Vectorize User Input → `true / false`
- Vectorize AI Response → `true / false`
- Temporary Chat TTL → `12 hr / 24 hr / 48 hr / 72 hr / 1 week / max`
- Active Chat Model → Select from downloaded chat models
- Active Vector Model → Select from downloaded embedding + vision models

**Examples:**
```bash
cluaiz permission
cluaiz permission firewall strict
cluaiz permission firewall auto
cluaiz permission telemetry off
```

---

## ⚡ BOOSTER COMMANDS

> Controls `system_booster.json` — hardware optimization settings.

| Command | Flag | Values | Description |
|---------|------|--------|-------------|
| `cluaiz booster` | _(no args)_ | | Open interactive booster TUI menu |
| `cluaiz booster --mode <mode>` | `--mode` | `edge / multitasking / balance / max_boost / ultra_max_boost / hyper_cluster` | Set performance profile |
| `cluaiz booster --kv-quant <level>` | `--kv-quant` | `auto / kv16 / kv8 / kv4` | Set KV-Cache quantization level |
| `cluaiz booster --context-shift <mode>` | `--context-shift` | `auto / off / minimal / standard / aggressive / extreme` | Set context shifting mode |
| `cluaiz booster --spec-decode <mode>` | `--spec-decode` | `on / off / auto` | Enable/disable speculative decoding |

**Examples:**
```bash
cluaiz booster
cluaiz booster --mode edge
cluaiz booster --mode max_boost
cluaiz booster --kv-quant kv8
cluaiz booster --context-shift aggressive
cluaiz booster --spec-decode on
cluaiz booster --mode edge --kv-quant kv8 --context-shift aggressive
```

---

## 🧩 COMPONENT MANAGEMENT COMMANDS (Extensions, Plugins, Skills, MCP)

> Manages the installation and lifecycle of all Sovereign AI Ecosystem components. 
> You can use the component type (`extension`, `plugin`, `skill`, `mcp`) or aliases (`ext`, `p`).

| Command | Aliases | Description |
|---------|---------|-------------|
| `cluaiz <type> install <id>` | `i` | Install a component from the cluaiz-hub registry (e.g., `cluaiz-search`) |
| `cluaiz <type> list` | `ls` | List all locally installed components of that type |
| `cluaiz <type> remove <id>` | `rm` | Remove an installed component |
| `cluaiz <type> start <id>` | | Start a component's background daemon (Extensions/MCP only) |
| `cluaiz skill cache clear` | `--all`, `--force`| Clear orphaned dual-caches for skills |

**Examples:**
```bash
# Install an extension
cluaiz extension install cluaiz-search
cluaiz ext i cluaiz-search

# List installed plugins
cluaiz plugin ls

# Remove an MCP
cluaiz mcp rm postgres-connector
```

---

## 📄 INGEST COMMANDS

> Natively ingest documents for semantic chunking and RAG.

| Command | Args | Description |
|---------|------|-------------|
| `cluaiz ingest <file-path>` | File path | Ingest a document (PDF, TXT, MD, etc.) for semantic chunking |

**Examples:**
```bash
cluaiz ingest ./document.pdf
cluaiz ingest "C:\Users\Aryan\Documents\notes.md"
```

---

## 🛠️ SETUP COMMANDS

| Command | Description |
|---------|-------------|
| `cluaiz setup profile` | Generate and register Purpose Vectorization for the Node Profile |

---

## 🌐 HTTP API (Port 8000 — when `cluaiz serve` is running)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/chat` | Chat with cluaiz (streaming) |
| `GET` | `/hardware` | System hardware status |
| `GET` | `/models/installed` | List installed models |
| `GET` | `/models/tags` | Full model roster |
| `POST` | `/models/load` | Load/activate a model |
| `POST` | `/models/download` | Download a model from HuggingFace |
| `POST` | `/v1/db/execute` | Execute a CDQL database query |
| `GET` | `/v1/permission` | Read Permission.json |
| `POST` | `/v1/permission/update` | Update a permission field |
| `POST` | `/v1/system/brain` | Toggle brain mode |
| `GET` | `/v1/system/control` | Read system_control.json |
| `GET` | `/v1/booster/status` | Read booster settings |
| `POST` | `/v1/booster/update` | Update booster settings |
| `POST` | `/v1/ingest/file` | Ingest a document |
| `GET` | `/health` | Health check ping |

---

## 🔌 IPC Named Pipe Commands (App ↔ Engine)

> Pipe: `\\.\pipe\cluaiz_engine_pipe` — used by Tauri Desktop App. JSON format.

| Action | Payload | Description |
|--------|---------|-------------|
| `GET_SETTINGS` | `{"action":"GET_SETTINGS"}` | Get all settings (permissions + booster + models) |
| `UPDATE_PERMISSION` | `{"action":"UPDATE_PERMISSION","payload":{"key":"...","value":"..."}}` | Update one Permission.json field |
| `UPDATE_BOOSTER` | `{"action":"UPDATE_BOOSTER","payload":{"key":"...","value":"..."}}` | Update one system_booster.json field |
| `BOOSTER_UPDATE` | `{"action":"BOOSTER_UPDATE","payload":{<full booster obj>}}` | Bulk booster update (CLI/legacy style) |
| `SYSTEM_BRAIN` | `{"action":"SYSTEM_BRAIN","payload":{"state":true}}` | Toggle brain mode on/off |
| `CDQL_FETCH_HISTORY` | `{"action":"CDQL_FETCH_HISTORY","session_id":"..."}` | Fetch chat history from LMDB |
| `CDQL_DELETE_SESSION` | `{"action":"CDQL_DELETE_SESSION"}` | Delete a session (pending) |
| `SYSTEM_PS` | `{"action":"SYSTEM_PS"}` | List active engine processes |
| `HARDWARE_CALIBRATE` | `{"action":"HARDWARE_CALIBRATE"}` | Re-calibrate hardware |
| `BENCHMARK_RUN` | `{"action":"BENCHMARK_RUN"}` | Start full benchmark |
| `MODEL_RM` | `{"action":"MODEL_RM","payload":{"model_id":"..."}}` | Remove a model file from vault |
| `SET_MODEL` | `{"action":"SET_MODEL",...}` | Hotswap model (stub — not yet wired) |
| `SET_HARDWARE` | `{"action":"SET_HARDWARE",...}` | Adjust compute device (stub) |
| `EAGER_LOAD` | `{"action":"EAGER_LOAD"}` | Pre-load model into memory (stub) |
| `SYSTEM_PROFILE_SETUP` | `{"action":"SYSTEM_PROFILE_SETUP"}` | Detect and write hardware profile |
| `<natural text>` | Plain string (no JSON) | Chat inference → token-by-token stream response |

---

## 📂 Config Files (`~/.cluaiz/engine/`)

| File | Purpose |
|------|---------|
| `Permission.json` | Privacy, active models, vectorization settings |
| `system_booster.json` | Hardware performance optimization profile |
| `system_control.json` | Hardware fingerprint, brain mode, OS identity |

---

## 📦 Model Vault Structure (`~/.cluaiz/models/`)

| Folder | Model Type | Appears In |
|--------|-----------|------------|
| `models/chat/` | LLM / Generative chat models | **Chat Model** dropdown only |
| `models/embedding/` | Text embedding / ONNX vector models | **Vector Model** dropdown only |
| `models/vision/` | Image CLIP / vision-embedding models (e.g. FashionCLIP, CLIP-ViT) | **Vector Model** dropdown (can embed images into vector space) |

> **Classification Logic (ffi_bridge.rs `GET_SETTINGS`):**  
> Primary = folder path (`/models/chat/` → chat, `/models/embedding/` or `/models/vision/` → vector)  
> Fallback = `category` field in `model_manifest.json` (`"chat"` → chat, `"embedding"/"vision"/"multimodal"` → vector)

---

## ⚙️ UNIVERSAL COMPONENT CONFIGURATION

The CLI provides an interactive, strict-schema mechanism to configure any component. It reads the component's `manifest-*.yaml` to understand the available `settings:` (their types, default values, and enum options), and then securely saves user overrides into `~/.cluaiz/engine/config/user_settings.yaml` without mutating the core files.

| Command | Action |
|---------|--------|
| `cluaiz config set` | Launches the **Interactive TUI Dropdown Menu** to select component type, component ID, and the setting to modify. |
| `cluaiz config set <type> <id> <setting_key> <value>` | One-shot command to set a value non-interactively. (e.g., `cluaiz config set extension cluaiz-search search_api_key "xxx"`) |

**Features of `config set`:**
- **Cute Dropdowns:** If a setting is defined as type `enum` in the manifest, the CLI renders an interactive `<inquire>` dropdown menu of allowed `options` instead of a blank text prompt.
- **Clean Keys:** You no longer type internal prefixes like `configuration_schema.` or `settings.`. Just select the direct setting name (e.g. `api_key`).
- **Strict Types:** String types are automatically safely serialized into YAML without causing formatting errors in the background config file.
