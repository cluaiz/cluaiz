# Cluaiz CLI Reference

What each `cluaiz` command does, and the API method that does the same thing over HTTP.
`cluaiz --help` and `cluaiz <command> --help` list every flag.

---

## Chat and agent

### `cluaiz` (no command)
* In a terminal, with a default chat model set: opens the agent in the current folder.
  Otherwise prints the help.

### `cluaiz agent ["task"]`
* Coding agent in this folder: reads, searches and edits files and runs commands, asking before
  changes. Reads the project's `AGENTS.md` and `.agents/` files. See [agent](../cmd/agent.md).
* Loads the model in this process; no server is needed.

### `cluaiz run [model] ["prompt"]`
* Chat in the terminal, or answer one prompt and exit. See [chat](../cmd/chat.md).
* `model` is an installed id, a `.gguf` path, or an id to download first. If the model is
  missing, `run` downloads it, then loads it.
* llama.cpp-style flags work: `-c`, `-ngl`, `-fa`, and more.
* API: `POST /v1/chat/completions` (or `/api/chat`).

---

## Models

> [!IMPORTANT]
> - `run` downloads a missing model and then loads it.
> - `pull` only downloads and registers it.
> - `rm` deletes the model's files from disk.

### `cluaiz pull <model>`
* Downloads a model (Hugging Face or the Cluaiz catalog) into `~/.cluaiz/models/` and registers
  it. In a terminal it asks which variant when a repository has several, then asks to go on.
* No questions (the first variant, no confirmation) when `CLUAIZ_NONINTERACTIVE` is set or
  stdin is not a terminal (scripts, CI, pipes). `--all` (pick files by hand) needs a terminal.
* API: CEP `models.download`, or `POST /api/pull` for Ollama clients.

### `cluaiz list` (`ls`)
* Installed models; `*` marks the default chat model.
* API: CEP `models.list`; `GET /v1/models`, `GET /api/tags`.

### `cluaiz show <model>`
* GGUF details, thinking tags, and the plan the model would get on this machine.
* API: CEP `models.inspect`, `system.plan`.

### `cluaiz rm <model>`
* Deletes a model's folder and its registry entry.
* API: CEP `models.remove`.

### `cluaiz model set-chat <id>` / `cluaiz model set-vector <id>`
* Sets the default chat model or embedding model in `permission.json`. `/v1/embeddings` and
  `cluaiz ingest` use the embedding model when a request names none.

### `cluaiz ps`
* The model the running server has loaded.
* API: CEP `system.ps`.

---

## Server

### `cluaiz serve`
* Starts the HTTP API on the host and port in `permission.json` (default port 8000).
* Routes: `/v1/chat/completions`, `/api/chat`, `/v1/embeddings`, `/v1/audio/speech`,
  `/v1/audio/transcriptions`, `/v1/ingest`, `/v1/models`, `/api/tags`, `/api/pull`, `/health`,
  CEP `POST /v1/cep` and MCP `/mcp`. See [API](../api/README.md).
* `--native-endpoint <pipe>`: a local pipe or socket instead of a TCP port.

### `cluaiz mcp-server`
* MCP over stdio for MCP clients (`"command": "cluaiz", "args": ["mcp-server"]`), backed by the
  running server's `/mcp`. Read-only methods only.

### `cluaiz token show|create|list|remove`
* The session token and API keys (at most 5).
* API: CEP `tokens.create`, `tokens.list`, `tokens.revoke`.

### `cluaiz permission [name value]`
* Without arguments: shows the values. With a name: changes one (`telemetry`, `lazy-load`,
  `port`, …). See [permission.json](../engine/permission.md).
* `firewall`, `kvcache`, `vectorize-input` and `vectorize-response` are saved but not used by
  the Engine yet.
* API: CEP `config.get` / `config.set` with `section: "engine"`.

---

## This machine

### `cluaiz status`
* The hardware the planner sees: CPU, RAM, GPUs and their free memory.
* API: CEP `system.info`.

### `cluaiz calibrate`
* Runs the hardware probe again (after a hardware or driver change) and saves
  `hardware_profile.json`. See [hardware profile](../engine/system_control.md).

### `cluaiz bench <model>`
* Prompt and generation speed, like llama-bench (pp512, tg128). Runs locally.
* `--placements` tests every placement that fits (full GPU, split, CPU only); the planner then
  loads the fastest one. See [measured speed](../engine/llm_optimization.md#measured-speed).

---

## Tools

Skills, plugins and MCP servers share one command shape. Aliases: `i` (install), `ls` (list),
`rm` (remove).

| Command | What it does | API |
|---|---|---|
| `cluaiz skill\|plugin\|mcp install <name>` | Download and install from the Cluaiz Tools registry | `tools.install` |
| `cluaiz skill\|plugin\|mcp list` | Installed ones | `tools.list` |
| `cluaiz skill\|plugin\|mcp remove <name>` | Remove one | `tools.remove` |
| `cluaiz skill\|plugin\|mcp cache …` | Cached files of a tool | `tools.cache`, `tools.clear_cache` |
| `cluaiz mcp start <name>` | Start an MCP server or extension in the background | — |
| `cluaiz plugin link …` | Link a plugin to a skill | — |
| `cluaiz skill\|plugin\|mcp config <name>` | View or change a tool's settings | `tools.get_settings`, `tools.set_settings` |

* **Skills** are instructions (`SKILL.md`) the model loads when it needs them.
* **Plugins** are programs. A plugin runs as a normal child process with your user's rights;
  there is no sandbox, so install only plugins you trust.
* **MCP servers** run over stdio and are started only when the model loads them.

---

## Documents and storage

### `cluaiz ingest <file>`
* Reads a document (PDF, DOCX, DOC, text, code) or an image and prints the text chunks. If an
  embedding model is set (`cluaiz model set-vector`), it also makes the vectors. Nothing is
  stored.
* API: `POST /v1/ingest`.

### `cluaiz storage status|clean`
* Temporary media of the running server.
* API: CEP `storage.status`, `storage.clean`.

---

## Settings

### `cluaiz config list|get|set|reset|path`
* `settings.json`. `set` accepts `auto` or `null` for the automatic value; `reset` backs the
  file up first. See [engine configuration](engine-configuration.md).
* API: CEP `config.get` / `config.set` (sections `gguf`, `onnx`, `optimization`, `context`,
  `storage`, `kv`, `engine`).

---

## Update and remove

### `cluaiz self update`
* **Usage:** `cluaiz self update [--check] [--to <version>]`
* Updates to the newest release (pre-releases included, as the installers do). `--check` only
  says whether one is out; `--to 0.1.2` installs that release, an older one too.
* It downloads that release's own installer (`install.ps1` / `install.sh` from the release tag)
  and runs it into the folder this `cluaiz` runs from, so the GPU pack is chosen and every file is
  checked against `SHA256SUMS.txt` as on a first install. Models and settings stay.
* On Windows the installer opens in its own window after the command exits, because a running
  `cluaiz.exe` and its DLLs are locked.
* It stops when a Cluaiz server (the App or `cluaiz serve`) is running, and it refuses a
  development build (one inside a Cargo workspace).
* API: CEP `system.update_check` (the check only; there is no update method in the API).

### `cluaiz self uninstall`
* **Usage:** `cluaiz self uninstall [--keep-models] [-y]`
* Shows what goes (the Cluaiz home `~/.cluaiz` or `CLUAIZ_HOME`, with the size of the models, and
  the program folder) and asks first. `-y` does not ask; without a terminal `-y` is required.
* Removes the PATH entry the installer added (Windows: the user PATH; Linux and macOS: the
  installer's `# Cluaiz` block in `.bashrc`, `.zshrc` and `.profile`, only when the line is
  unchanged), then the files. `--keep-models` keeps `models/` and the model list.
* A running Cluaiz server must be stopped first. The Cluaiz App is a separate program and is not
  removed.
* API: none, on purpose: a remote caller must not be able to delete the Engine.

---

## Development

### `cluaiz dev-sync`
* Copies development builds (core, drivers) into `~/.cluaiz`. For Engine developers only.
