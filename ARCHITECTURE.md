# Cluaiz Engine Architecture

This page describes how the Engine is built today. It lists only what is in the code. Planned
work is marked as planned.

## 1. Ways in

| Entry | What it is | Code |
|---|---|---|
| `cluaiz` CLI | All commands: `run` (chat), `agent` (terminal coding agent), `pull`, `serve`, `config`, … | `cmd/` |
| HTTP server | `cluaiz serve`: 11 routes. OpenAI- and Ollama-style chat, embeddings, audio, ingest, models, `/v1/cep`, `/mcp` | `api/src/routes.rs` |
| Local pipe or socket | `cluaiz serve --native-endpoint <pipe>`: no TCP port, session token required | `api/src/native/` |
| C library | `cluaiz_cep.dll` / `libcluaiz_cep.so` with `cep_call(json)` and `cep_free` | `ffi/` |
| MCP | `/mcp` over HTTP and `cluaiz mcp-server` over stdio; read-only CEP methods only | `api/src/cep/mcp.rs` |

New features are added as CEP methods (JSON-RPC 2.0, names are `group.verb`), not as new REST
routes. See [docs/api/cep/README.md](docs/api/cep/README.md).

## 2. Crates

| Crate | Folder | Job |
|---|---|---|
| `cmd` | `cmd/` | The `cluaiz` program: CLI commands, terminal agent, interactive chat |
| `cluaiz-api` | `api/` | HTTP server, chat pipeline (preflight, tool loop, recall), CEP methods |
| `cluaiz-cep` | `cep/` | JSON-RPC envelope, method registry, scopes, approvals, tasks, OpenRPC |
| `cluaiz-agents` | `agents/` | Installed tools (skills, plugins, MCP), tool search, chat memory store, notes, `.agents/` workspace files |
| `cluaiz-context` | `context/` | Context planner, Chat Map (cards, BM25 recall), compaction, context policy |
| `cluaiz-security` | `security/` | `permission.json` schema, API tokens, audit log |
| `cluaiz-terminal` | `terminal/` | TUI library used by the terminal agent |
| `cluaiz-ffi` | `ffi/` | The C library |
| `cluaiz-core` | `engine/core/` | Settings, paths, managed prompts, chat and media types |
| `cluaiz-runtime` | `engine/runtime/` | `EngineHost`: model load and unload, KV cache save and restore, media admission, memory sharing between models |
| `cluaiz-plan` | `engine/plan/` | Planner: how a model is split across GPU, RAM and disk on this machine |
| `cluaiz-hardware` | `engine/hardware/` | CPU, RAM and GPU probe |
| `cluaiz-models` | `engine/models/` | Model registry, downloads, GGUF and ONNX probing |
| `llama-engine` | `engine/llama/` | Rust bindings to llama.cpp (GGUF): chat, multimodal, speech, embeddings |
| `onnx-engine` | `engine/onnx/` | ONNX Runtime: TTS, STT, text and image embeddings, CUDA/cuDNN setup |
| `cluaiz-media` | `engine/media/` | Image, audio, video and document decoding |
| `cluaiz-automation` | `automation/core/` | Automation, outside the Engine: the flow format, checks, expressions, runner and run records |
| `cluaiz-automation-host` | `automation/host/` | Runs Automation next to the server: the `flows.*` methods, and steps that reach the Engine only through CEP methods and the chat pipeline |

## 3. How a chat request runs

1. **Route.** `POST /v1/chat/completions` (or `/api/chat`) reaches `api/src/handlers/chat/`.
2. **Preflight** (`preflight.rs`) builds the prompt in zones:
   - the system prompt and managed prompts (`engine/core/prompts/*.md`);
   - the project's agent files (`AGENTS.md`, `.agents/rules`, memory), when `project_files` is on;
   - chat history from the event log, the Chat Map recall and the confirmed notes;
   - the cards of the installed tools that fit the message (`agents/src/tool_search.rs`).
3. **Placement.** `EngineHost` loads the model with the plan from `engine/plan` (GPU layers, KV
   cache size, offload), or reuses the loaded one.
4. **Generation.** llama.cpp generates. Tool calls go through the agent turn core
   (`agents/src/turn.rs`), the same one the terminal agent uses; the API's part is
   `api_turn.rs` (tools, approvals, SSE chunks) and `tool_turns.rs`. Calls that need approval wait
   for the owner.
5. **Save.** The turn is written to the event log; the KV cache is saved so the next turn reuses
   the shared prefix.

When the context fills up, compaction replaces old turns with a summary and keeps the recall index.

## 4. Memory

There is no vector database and no graph database. Chat memory is:

- an append-only event log per chat;
- the Chat Map: cards per turn, searched with BM25 word matching;
- notes: facts the model proposes and the user confirms; project facts move to
  `.agents/memory/<name>.md` in the project folder;
- the saved KV cache of each chat.

Vector embeddings are available through `/v1/embeddings`, but chat memory does not use them:
in our tests they matched worse than word matching.

## 5. Models and hardware

- GGUF models run through llama.cpp, linked into the program. GPU support comes from the DLLs or
  shared libraries next to the executable (CPU core build plus CUDA 12, CUDA 13 or Vulkan packs).
- ONNX models run through ONNX Runtime (TTS, STT, embeddings).
- The planner reads the hardware profile and the model's size and decides how many layers go on
  the GPU, the KV cache size, and when idle models are unloaded to make room.

## 6. Not built

- No WASM or OS sandbox for plugins: a plugin runs as a normal child process
  (`agents/src/plugins/executor.rs`).
- Some `permission.json` settings do nothing (`wasm_firewall`, `vectorize_user_input`,
  `vectorize_ai_response`, `enable_kvcache`, `vector_models.vision`, `vector_models.audio`). They are
  kept for existing files and the App, and CEP `config.get` lists them as `engine.unused_fields`.
- The API chat and the terminal agent have separate tool loops.

## More

- Engine crates: [engine/README.md](engine/README.md)
- Chat API: [docs/api/inference/chat_completions.md](docs/api/inference/chat_completions.md)
- Terminal agent: [docs/cmd/agent.md](docs/cmd/agent.md)
- Install layout: [docs/deployments/install-layout.md](docs/deployments/install-layout.md)
