# CEP: Cluaiz Engine Protocol

CEP carries every Cluaiz-specific Engine operation, all in the same message shape: [JSON-RPC 2.0](https://www.jsonrpc.org/specification).

| Reach it through | Who it is for |
|---|---|
| `POST /v1/cep` | The App, scripts and other programs |
| `cep_call` / `cep_free` | Programs that load the Engine as a library (`cluaiz_cep.dll`) |
| `POST /mcp` and `cluaiz mcp-server` | MCP clients such as Claude Desktop and Cursor |

Market-standard routes stay as they are: `/v1/chat/completions`, `/api/chat`, `/v1/embeddings`, `/v1/audio/speech`, `/v1/audio/transcriptions`, `/v1/models`, `/api/tags`, `/api/pull`, `/health`.

- **Every method**, with its params, scope and an example: [methods.md](methods.md).
- **The same list, machine-readable:** [openrpc.json](openrpc.json). Both are generated from the Engine.

## A call

```json
→ {"jsonrpc":"2.0","id":1,"method":"fs.read","params":{"path":"notes.txt"}}
← {"jsonrpc":"2.0","id":1,"result":{"path":"D:/work/notes.txt","size":5,"binary":false,"text":"hello"}}
```

- **`method`** is `group.verb`: `fs.read`, `memory.events`, `tools.clear_cache`.
- **`params`** is always an object, by name. An unknown or misspelt field is an error, never silently ignored.
- **`params._meta`** carries protocol data: `approval` (an approval id) and `trace` (your own id for logs).
- **`result`** is the data itself, with no `status` / `success` / `message` wrapper. Success is the presence of `result`.
- **A notification** is a request with no `id`. It runs, and nothing is answered.
- **A batch** is a JSON array of up to 32 requests. They run in order, and the answers come back in order. Notifications in a batch get no answer; an all-notification batch gets HTTP 202 with no body.

HTTP:
- `Content-Type: application/json` is required (415 otherwise).
- Authentication follows the Engine's `api_auth.required` setting, like every other route. When it is on, send `Authorization: Bearer <token>`; the token is in `<home>/session.token` (default home `~/.cluaiz`) or is an API key.
- Every JSON-RPC answer is HTTP 200, errors included. 401 and 413 come from authentication and the body size limit.

### Discovery

```json
{"jsonrpc":"2.0","id":1,"method":"rpc.discover","params":{"group":"notes"}}
```

The answer is an [OpenRPC](https://spec.open-rpc.org/) document, listing only the methods the caller may call. Without `group`, it lists every method. For each method it gives:
- the params schema
- an example
- the extension `x-cep`: `scope`, `execution`, `destructive`, `mcp`, `owner_only`

## Errors

| Code | `data.type` / meaning | When |
|---|---|---|
| -32700 | parse error | The body is not JSON (or not UTF-8) |
| -32600 | `invalid_request` | Not a request; empty batch; batch over 32; nesting over 64 |
| -32601 | method not found | `data.did_you_mean` lists close names; `data.group_methods` lists the group's methods |
| -32602 | invalid params | `data.errors[]` has `path` and `message`; `data.example` has the method's example params |
| -32603 | internal error | `data.trace_id` points into the Engine log |
| -32001 | `unauthenticated` | No valid token while authentication is required |
| -32002 | `forbidden` | `data.needs`: the missing scope, `roots` or `owner` |
| -32003 | `approval_required` | `data.approval_id`: see [Approvals](#approvals) |
| -32004 | `*_not_found` | A chat, file, model, note, task, tool… |
| -32005 | conflict | Busy or wrong state (for example `reasoning_unavailable`) |
| -32006 | `too_large` | `data.limit`, `data.got` |
| -32007 | `not_implemented` | Declared, not built yet (never a fake success) |
| -32008 | `cancelled` | |
| -32009 | `timeout` | `tasks.result` waited `wait_ms` and the task still runs |
| -32010 | unavailable | A backend is missing or failed |

## Long work: tasks

These methods answer at once with `{"task": {"task_id", "status": "working", …}}`:
- `ingest.run`
- `audio.run`
- `tools.install`
- `chat.import_app`
- `models.download` (`-32007` for now)

| Method | Does |
|---|---|
| `tasks.get {task_id, after?}` | Status, `progress`, and the `events` after cursor `after` (one per ingested source, audio chunks…) |
| `tasks.result {task_id, wait_ms?}` | The final result or error, exactly what a direct call would give. Waits up to `wait_ms` (at most 30 000), then `-32009` while still running. |
| `tasks.cancel {task_id}` | Stops it (`cancelled`) |
| `tasks.list` | Your tasks that have not expired |

The statuses match MCP tasks: `working`, `completed`, `failed`, `cancelled`. A task belongs to its caller and lives in memory for an hour.

## Approvals

A call that needs the owner's yes fails with `-32003` and an `approval_id`. The same store holds the chat agent's tool approvals, whose SSE `permission_request.request_id` is the same id.

1. The owner answers with `approvals.approve {approval_id, remember?: "chat", minutes?}` or `approvals.reject {approval_id, feedback?}`.
2. To retry, repeat the **same** call with `"_meta": {"approval": "<id>"}`.

An approval covers exactly that caller, method and params. It is used once and expires after 5 minutes.

## Examples

**curl**

```bash
TOKEN=$(cat ~/.cluaiz/session.token)
curl -s http://127.0.0.1:8000/v1/cep -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.info"}'
```

**Python**

```python
import json, pathlib, urllib.request

TOKEN = (pathlib.Path.home() / ".cluaiz" / "session.token").read_text().strip()

def cep(method, **params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    request = urllib.request.Request("http://127.0.0.1:8000/v1/cep", body,
                                     {"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    reply = json.load(urllib.request.urlopen(request))
    if "error" in reply:
        raise RuntimeError(reply["error"])
    return reply["result"]

task = cep("ingest.run", source="report.pdf")["task"]
print(cep("tasks.result", task_id=task["task_id"], wait_ms=30000))
```

**Node.js**

```js
const TOKEN = (await import("node:fs")).readFileSync(`${(await import("node:os")).homedir()}/.cluaiz/session.token`, "utf8").trim();
async function cep(method, params = {}) {
  const reply = await fetch("http://127.0.0.1:8000/v1/cep", {
    method: "POST", headers: { "content-type": "application/json", authorization: `Bearer ${TOKEN}` },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  }).then((r) => r.json());
  if (reply.error) throw Object.assign(new Error(reply.error.message), reply.error);
  return reply.result;
}
```

## The Engine inside your program: `cep_call` / `cep_free`

```c
char *cep_call(const char *request_json);  /* the reply JSON, or NULL for notifications only */
void  cep_free(char *reply);
```

- **Library:** `cluaiz_cep.dll` / `libcluaiz_cep.so` / `libcluaiz_cep.dylib`; header `ffi/include/cep.h`.
- **Messages:** the same as `/v1/cep`.
- **Start-up:** the first call starts the Engine with `CLUAIZ_HOME` (or the default home). It does not touch `session.token`, so a `cluaiz serve` on the same home keeps working.
- **Caller:** the owner (the calls come from your process).
- **Windows, CUDA build:** add `CUDA_PATH\bin` with `os.add_dll_directory` before loading from Python.
- **More:** `ffi/README.md`.

## MCP

- **`POST /mcp`** is an MCP server (Streamable HTTP, JSON answers, protocol 2025-11-25; 2025-06-18 and 2025-03-26 also accepted).
  - Its tools are the CEP methods with the same names, and `inputSchema` is the params schema.
  - MCP clients get the **read-only** methods only.
  - An error from the method comes back as a tool result with `isError: true`, so the model can correct its call.
- **`cluaiz mcp-server`** is the same server over stdio, for clients that start one. Claude Desktop config: `{"command": "cluaiz", "args": ["mcp-server"]}`. It talks to the running `cluaiz serve`.

## From the old routes

| Old route | CEP |
|---|---|
| `/info`, `/hardware`, `/v1/system/info`, `/v1/system/control`, `/v1/system/profile`, `/v1/hardware/calibrate` | `system.info` |
| `/v1/system/live`, `/v1/system/ps`, `/v1/system/plan` | `system.live`, `system.ps`, `system.plan` |
| `/v1/system/cmd` | `system.exec {command}` |
| `/v1/system/storage/temp_media`, `…/clean` | `storage.status`, `storage.clean` |
| `/v1/system/gguf_config`, `/v1/system/onnx_config`, `/v1/optimization/*`, `/v1/context/settings`, `/v1/system/storage/settings`, `/v1/chat/memory/settings`, `/v1/system/permission` | `config.get {section?}`, `config.set {section, patch}` (sections `gguf`, `onnx`, `optimization`, `context`, `storage`, `kv`, `engine`) |
| `/v1/system/auth/token/generate`, `…/revoke` | `tokens.create`, `tokens.revoke` (`tokens.list`) |
| `/v1/system/permission/pending`, `…/approve`, `…/reject` | `approvals.list`, `approvals.approve`, `approvals.reject` |
| `/v1/fs/{read,write,list,delete,rename,copy,mkdir,pick_folder}`, `/v1/workspace` | `fs.*`, `workspace.get`, `workspace.set` (renamed fields: `content` → `text`/`data_base64`, `dir_path` → `path`, `old_path`/`new_path` and `src_path`/`dest_path` → `from`/`to`) |
| `/models/available`, `/v1/models/installed`, `/models/load`, `/v1/models/{id}/inspect_raw_header`, `DELETE /v1/models/{id}`, `/models/download` | `models.list`, `models.load`, `models.inspect`, `models.remove`, `models.download` |
| `/v1/tools`, `/v1/tools/discover`, `/v1/{skills,plugins,mcp}/{list,install,remove,cache}`, `/api/components/*`, `/v1/execute/{c}/{f}` | `tools.*` (`kind` = `skill` \| `plugin` \| `mcp`) |
| `/v1/chat/cancel`, `/v1/chat/skip-reasoning`, `/v1/chat/context_telemetry`, `/v1/chat/{id}/tools`, `/v1/chat/import/app` | `chat.cancel`, `chat.skip_reasoning`, `chat.telemetry`, `chat.tools` / `chat.set_tools`, `chat.import_app` |
| `/v1/chat/{id}/events`, `…/index`, `…/export`, `…/import`, `…/history` (DELETE), `…/grants`, `/v1/recall/search`, `/v1/recall/item/{ref}` | `memory.events`, `memory.index`, `memory.export`, `memory.import`, `memory.delete`, `memory.grants` / `memory.revoke_grants`, `memory.search`, `memory.item` |
| `/v1/notes`, `/v1/notes/{id}/{confirm,invalidate,pin}` | `notes.list`, `notes.create`, `notes.confirm`, `notes.invalidate`, `notes.pin` |
| `/v1/chat/{id}/context/preview`, `…/context/report[/{request_id}]`, `/v1/context/jobs` | `context.preview`, `context.reports` / `context.report`, `context.jobs` |
| `/v1/chat/memory` (GET, DELETE), `/v1/chat/memory/{id}` | `kv.status`, `kv.clear`, `kv.delete` |
| `/v1/audio/voices`, `/v1/audio/languages`, `/v1/audio/execute` | `audio.voices`, `audio.languages`, `audio.run` (task) |
| `/v1/ingest`, `/v1/ingest/file` | `ingest.run` (task; `namespace` removed) |
| `/v1/control`, `/v1/control/batch`, `/v1/control/ops`, `/v1/control/execute`, `/v1/mcp/rpc` | CEP itself: the envelope, batch, `rpc.discover`; `/mcp` |
| — (new) | `system.audit` (audit log), `tasks.*`, `rpc.discover` |

Path ids become params: `/v1/chat/c1/events?from_seq=40` → `memory.events {"chat_id":"c1","from_seq":40}`. A chat's id is `chat_id` everywhere (the `session_id` of chat completions).
