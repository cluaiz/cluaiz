# permission.json

`permission.json` is in the config folder (`~/.cluaiz/config/permission.json`, or
`<CLUAIZ_HOME>/config/permission.json`). The schema is `security/src/permission_schema.rs`.
Missing fields take their defaults. If the file cannot be parsed, the Engine uses the defaults.

Change it with `cluaiz permission …`, `cluaiz token …`, `cluaiz model set-chat|set-vector`, or
from the App settings screen.

## Fields the Engine uses

| Field | Default | What it does |
|---|---|---|
| `active_slots` | `{}` | The model chosen for each slot (`chat_slot`, `embed_slot`, `vision_slot`, `audio_slot`). A slot wins over `chat_models` / `vector_models` |
| `chat_models.text` | none | Default chat model (`model: "auto"` in a chat request) |
| `vector_models.text` | none | Default embedding model for `/v1/embeddings` when the request names none |
| `stream_telemetry` | `false` | Adds usage and timing details to chat responses |
| `model_header_info` | `false` | With `stream_telemetry`, also adds which models are loaded |
| `lazy_load_model` | `true` | `true`: load the chat model on the first request. `false`: start loading it when the server starts (the port opens without waiting) |
| `api_port` | `8000` | HTTP port of `cluaiz serve` |
| `api_host` | `0.0.0.0` | Address the server listens on. The `CLUAIZ_HOST` environment variable wins |
| `connection_protocol` | `http` | Shown by `cluaiz serve` |
| `api_auth.required` | `false` | `true`: every request needs a bearer token (the session token or an API key) |
| `api_auth.tokens` | `[]` | API keys, at most 5 (`cluaiz token create|list|remove`) |
| `agent_security_mode` | `sandboxed` | Approval rule for tool calls, used by the App, the API chat and the terminal agent: `full_access`, `sandboxed` (workspace-gated) or `strict` |
| `exec.allow_shell` | `false` | CEP `system.exec` also accepts a shell line, not only a program with arguments |
| `exec.without_approval` | `false` | CEP `system.exec` runs without the owner's approval of each call |

## Fields that do nothing

Existing files and the App settings screen may still hold these, so they are read and saved, but
no Engine code acts on them and `cluaiz permission` no longer shows or changes them. CEP
`config.get` lists them under `engine.unused_fields`, so a settings screen can hide them or mark
them as not working:

| Field | Default | Why it does nothing |
|---|---|---|
| `wasm_firewall` | `auto` | Not read. A `.wasm` plugin always runs closed in, with limits (`agents/src/plugins/wasm.rs`); any other plugin runs as a normal child process (`agents/src/plugins/executor.rs`) |
| `vectorize_user_input` | `true` | Messages are not vectorized. Notes use the policy `notes_match` instead |
| `vectorize_ai_response` | `true` | Answers are not vectorized |
| `enable_kvcache` | `true` | Saving a chat's KV cache is controlled by `settings.json` → `chat_memory.enabled` |
| `vector_models.vision`, `vector_models.audio` | none | No request path reads them |

## Related

- Execution policy for commands: `agents/src/execution/policy.rs` (destructive commands are always refused).
- Audit log of write, exec and admin calls: `security/src/audit.rs`, CEP `system.audit`.
- Security overview: [security-and-auth-architecture.md](../architecture/security-and-auth-architecture.md).
