# cluaiz CLI — Command Reference

> Binary: `cluaiz`. Built from `cmd/`. `cluaiz <command> --help` lists every flag.
> What each command does in more detail: [terminal commands](../reference/terminal-commands.md).

---

## Core

| Command | Description |
|---------|-------------|
| `cluaiz` | In a terminal with a default chat model: the [agent](agent.md) in the current folder. Otherwise the help |
| `cluaiz --help`, `cluaiz -h`, `cluaiz help` | Every command in groups; `cluaiz <command> --help` for its flags |
| `cluaiz -v`, `-V`, `--version` | The version (`-v` with a command means verbose: engine logs on stderr, `-vv` debug) |

---

## Chat and agent

| Command | Flags / Args | Description |
|---------|-------------|-------------|
| `cluaiz run` | _(no args)_ | Chat with the default chat model |
| `cluaiz run <model-id>` | load / sampling / thinking flags | Chat with a model; downloads it first when it is not installed ([guide](chat.md)) |
| `cluaiz run <model-id> "prompt"` | `-p`, `-f`, `-i`, `-n` | Answer one prompt and exit (`-i`: keep chatting). Piped stdin is the prompt |
| `cluaiz agent` | `-m <model>`, `-C/--dir <path>`, `--ui auto\|plain\|tui` | Coding agent in the current folder (or `--dir`): reads, searches and edits files, runs commands |
| `cluaiz agent "task"` | `-p` | Do one task and exit. Piped stdin is the task |
| `cluaiz agent --yes` | `-y` | Allow every change and command without asking (destructive commands stay refused) |

Inside the agent: `/yes`, `/ask`, `/clear`, `/undo`, `/rewind`, `/bye`, `/?`, and the project's
workflows as `/<name>`. Tools, approvals and limits: [agent guide](agent.md).

```bash
cluaiz run gemma4:e2b
cluaiz run bonsai:8b "Summarise this in one line" < notes.txt
cluaiz agent
cluaiz agent -m qwen3 --dir ../website "find where the footer year is set and update it to 2026"
cat task.txt | cluaiz agent --yes
```

---

## Models

| Command | Description |
|---------|-------------|
| `cluaiz pull <model-id>` | Download and register a model (no questions when `CLUAIZ_NONINTERACTIVE` is set or stdin is not a terminal) |
| `cluaiz list` (`ls`) | Installed models; `*` = default chat model |
| `cluaiz show <model-id>` | GGUF details and the plan the model would get on this machine |
| `cluaiz rm <model-id>` | Delete a model and its registry entry |
| `cluaiz model set-chat <model-id>` | Default chat model (`permission.json`) |
| `cluaiz model set-vector <model-id>` | Default embedding model (`permission.json`), used by `/v1/embeddings` and `cluaiz ingest` |
| `cluaiz ps` | The model the running server has loaded |

```bash
cluaiz pull qwen3:8b
cluaiz pull unsloth/Qwen3.5-4B-GGUF
cluaiz show qwen3:8b
cluaiz rm gemma4:e2b
cluaiz model set-chat gemma4:e2b
```

---

## Server, permissions and tokens

| Command | Description |
|---------|-------------|
| `cluaiz serve` | Start the API on `http://<api_host>:<api_port>` (`--native-endpoint <pipe>`: a local pipe or socket instead) |
| `cluaiz mcp-server` | MCP over stdio for MCP clients; forwards to the running Engine's `/mcp` (read-only methods) |
| `cluaiz permission` | Show the `permission.json` values |
| `cluaiz permission port <number>` | API port |
| `cluaiz permission telemetry on\|off` | Usage details in chat responses |
| `cluaiz permission lazy-load on\|off` | Load the chat model on first use, or at server start |
| `cluaiz token show` (`--all`) | The active API token and whether auth is on |
| `cluaiz token create` | New `sk-cluaiz-…` API key (at most 5) |
| `cluaiz token list` (`ls`) | All API keys |
| `cluaiz token remove <token>` (`rm`) | Remove a key by its full text or its suffix |

```bash
cluaiz token create
cluaiz token remove 7898
```

## Model providers

| Command | Description |
|---------|-------------|
| `cluaiz provider add <name> --url <base_url>` | Add or change a provider. `--key-env <VAR>`: the key's environment variable; `--key <key>` (or `-` for stdin): keep the key in providers.json; `--model <id>[=<context>]`: list a chat model; `--embedding`, `--tts`, `--stt <id>`: list a model of that kind; `--type anthropic`: the Anthropic Messages format at a URL the Engine does not know; `--auth bearer\|api-key\|x-api-key\|google\|aws\|azure\|none`: how the key is sent, when the URL does not tell |
| `cluaiz provider list` (`ls`) | The providers, where each key comes from, and their listed models |
| `cluaiz provider models <name> [filter]` | The models the provider offers, with their context size |
| `cluaiz provider test <name> [model]` (`--voice`) | One short request of the model's kind: the URL, the key and the model work |
| `cluaiz provider remove <name>` (`rm`) | Remove a provider |

A provider's model is used as `<provider>/<model>`: `cluaiz run myapi/model-name`, `cluaiz agent -m myapi/model-name`. See [Model providers](../engine/providers.md).

```bash
cluaiz provider add myapi --url https://api.example.com/v1 --key-env MYAPI_KEY
cluaiz provider test myapi model-name
```

---

## This machine

| Command | Flags | Description |
|---------|-------|-------------|
| `cluaiz status` | | The hardware the planner sees, and the GPU backend that loaded (`backend`) |
| `cluaiz calibrate` | | Re-scan the hardware (after a hardware or driver change) |
| `cluaiz bench <model-id>` | `-p`, `-n`, `-r`, `--placements`, `-o md\|json`, load flags (`-c`, `--n-gpu-layers`, `--n-cpu-moe`, `--cache-type-k`…) | Prompt and generation speed (llama-bench: pp512, tg128); `--placements` tests every placement and the planner then loads the fastest |
| `cluaiz self update` | `--check`, `--to <version>` | Update to the newest release with that release's installer; models and settings stay |
| `cluaiz self uninstall` | `--keep-models`, `-y` | Remove Cluaiz: its home, its program folder and its PATH entry |

---

## Tools (skills, plugins, MCP)

| Command | Alias | Description |
|---------|-------|-------------|
| `cluaiz <type> install <id>` | `i` | Install from the Cluaiz Tools registry (`<type>` = `skill`, `plugin` or `mcp`) |
| `cluaiz <type> list` | `ls` | Installed components of that type |
| `cluaiz <type> remove <id>` | `rm` | Remove an installed component |
| `cluaiz <type> cache …` | | Cached files of a component |
| `cluaiz <type> config <id>` | | View or change a component's settings (asks for missing values) |

A WebAssembly (`.wasm`) plugin runs inside the Engine and reaches only the time and the OS name. Any other plugin runs as a normal child process with your user's rights.

```bash
cluaiz plugin install cluaiz-search
cluaiz plugin ls
cluaiz mcp rm sqlite-bridge
```

---

## Documents, storage and settings

| Command | Description |
|---------|-------------|
| `cluaiz ingest <file-path>` | Read a document (PDF, DOCX, DOC, text, code) or image and print its chunks; with an embedding model set, also make vectors. Nothing is stored |
| `cluaiz storage status\|clean` | Temporary media of the running server |
| `cluaiz config list\|get\|set\|reset\|path` | `settings.json` ([engine configuration](../reference/engine-configuration.md)) |

```bash
cluaiz ingest ./document.pdf
cluaiz config get optimization.n_gpu_layers
cluaiz config set optimization.n_gpu_layers 20
cluaiz config set optimization.n_gpu_layers auto
```

---

## Development

These are not `cluaiz` commands. They run in the repository.

| Command | Description |
|---------|-------------|
| `cargo build --release -p cmd` | Build the `cluaiz` program |
| `cargo build --release -p cmd -p cluaiz-ffi` | Also build the C library |
| `cluaiz dev-sync` | Copy development builds (core, drivers) into `~/.cluaiz` |

---

## HTTP API (host and port are the owner's settings; default port `8000`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/v1/ingest` | Direct document/image extraction with optional selected-model processing |
| `POST` | `/v1/cep` | Engine control operations, as JSON-RPC 2.0 methods (`system.info`, `models.load`, `config.set`, `fs.*`, `memory.*`, …). See [docs/api/cep](../api/cep/README.md) |
| `POST` | `/mcp` | The same methods for MCP clients (read-only) |
| `POST` | `/v1/chat/completions`, `/api/chat` | Chat (OpenAI / Ollama), streaming |
| `POST` | `/v1/embeddings` | Embeddings (OpenAI) |
| `POST` | `/v1/audio/speech`, `/v1/audio/transcriptions` | Text-to-speech and speech-to-text (OpenAI) |
| `GET` | `/v1/models`, `/api/tags` | Installed models (OpenAI / Ollama) |
| `POST` | `/api/pull` | Ollama pull: downloads and registers a model, as `cluaiz pull` does (CEP: `models.download`) |
| `GET` | `/health` | Health check, always open |

```bash
curl -X POST http://localhost:8000/v1/cep -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.info","params":{}}'
```

Add `-H 'Authorization: Bearer <token>'` when API authentication is on.

---

## In-process apps

An app that embeds the Engine calls the same methods through the C library (`cluaiz_cep`:
`cep_call` / `cep_free`, header `ffi/include/cep.h`), with the same JSON-RPC text as `/v1/cep`.
See [C library and FFI](../reference/c-pointer-fii.md).

---

## Config files (`~/.cluaiz/config/`)

| File | Purpose |
|------|---------|
| `settings.json` | Engine settings: load and speed, samplers, media limits, chat memory, context |
| `permission.json` | Port, host, API auth, security mode, default models |
| `hardware_profile.json` | The saved hardware probe |
| `model_registry.json` | Installed models |
| `tools_registry.json` | Installed skills, plugins and MCP servers |

Details: [engine configuration](../reference/engine-configuration.md).
