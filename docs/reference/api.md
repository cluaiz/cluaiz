# Cluaiz API Reference

The Engine has three ways in, and they all run the same code:

| Way in | For | Reference |
|---|---|---|
| HTTP on the owner's host and port (default `8000`) | Apps, scripts, OpenAI / Ollama clients | [API overview](../api/README.md) |
| `POST /v1/cep`: CEP, JSON-RPC 2.0 | Every Cluaiz operation: system, settings, models, files, tools, chat memory, notes, context, audio tasks, ingest, approvals | [CEP](../api/cep/README.md), [methods by category](../api/cep/README.md#the-methods), [openrpc.json](../api/cep/openrpc.json) |
| `/mcp` and `cluaiz mcp-server` | MCP clients (read-only methods) | [CEP → MCP](../api/cep/README.md) |
| C library `cluaiz_cep` (`cep_call` / `cep_free`, `ffi/include/cep.h`) | Apps that embed the Engine in their own process | [CEP → C library](../api/cep/README.md) |

## Routes

| Method | Route | Reference |
|---|---|---|
| `GET` | `/health` | [health](../api/system/health.md) |
| `POST` | `/v1/cep` | [CEP](../api/cep/README.md) |
| `POST` | `/mcp` | [CEP → MCP](../api/cep/README.md) |
| `POST` | `/v1/chat/completions`, `/api/chat` | [chat completions](../api/inference/chat_completions.md) |
| `POST` | `/v1/embeddings` | [embeddings](../api/inference/embeddings.md) |
| `POST` | `/v1/audio/speech` | [speech](../api/inference/audio_speech.md) |
| `POST` | `/v1/audio/transcriptions` | [transcriptions](../api/inference/audio_transcriptions.md) |
| `GET` | `/v1/models`, `/api/tags` | [list models](../api/models/list_models.md) |
| `POST` | `/api/pull` | [pull](../api/models/pull_model.md) |

Every route except `/health` follows the owner's `api_auth.required` setting: when it is on, send `Authorization: Bearer <token>`. See [Security & Auth Architecture](../architecture/security-and-auth-architecture.md).

Each CEP method has its own page under [`docs/api/cep/`](../api/cep/README.md), in five categories. The `---` block at the top of each page is generated from the Engine's method registry (`api/tests/cep_docs.rs`), so it is what the running Engine accepts.
