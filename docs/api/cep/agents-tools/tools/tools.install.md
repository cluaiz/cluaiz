---
method: "tools.install"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Install a registry tool; task result reports the actual outcome"
scope: "write"
execution: "task"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "skill"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "registry-tool"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.install",
    "params": {
      "kind": "skill",
      "name": "registry-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "task": {
        "status": "working",
        "task_id": "t_example"
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "tool_operation_failed"
    when: "The installer, registry or tool itself failed (its message is passed on)"
  - code: -32008
    type: "cancelled"
    when: "`tasks.cancel` stopped it before or after the download (from the task's result)"
---

# tools.install

Install a named registry tool through the existing installer as a CEP task. Tool package resolution/runtime setup remain owned by ToolsInstaller; do not treat the returned task id as proof the package is ready.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tools.install` |
| Execution | Task: the first result is a handle; use tasks.get / tasks.result for output |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Validate kind/name and authorize the installer’s real category target
2. Start installation and emit installing/installed task events
3. Verify the installed entry and return it as the final task result

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Validate kind/name and authorize the installer’s real category target"]
    P0 --> P1["Start installation and emit installing/installed task events"]
    P1 --> P2["Verify the installed entry and return it as the final task result"]
    P2 --> R["Task handle, then final domain result"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `kind` | "skill" \| "plugin" \| "mcp" | Yes | skill, plugin or mcp Required enum: skill (instructions), plugin (WASM/native package), mcp (server package). Kind and name must identify the same installed entry. |
| `name` | string | Yes | The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens Required package/registry name, 1–64 ASCII lowercase letters/digits/dots/underscores/hyphens; first character must be a letter or digit. Not a path or arbitrary repository URL. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools.install",
  "params": {
    "kind": "skill",
    "name": "registry-tool"
  }
}
```

### 2. Install an MCP registry package

Replace with a real tool name available to this installer; this does not accept an arbitrary shell command.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools.install",
  "params": {
    "kind": "mcp",
    "name": "registry-server"
  }
}
```

## Response and field meanings

Initial result is a task handle. The final result is the installed ToolEntry after the owner succeeds, not merely {success:true}. Stage events identify kind/name. Installer exceptions are tool_operation_failed.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "task": {
      "status": "working",
      "task_id": "t_example"
    }
  }
}
```

### Read the task result

Copy the returned `result.task.task_id`. Task state is in memory and is caller-owned; it is not a durable job queue. Poll for events with the returned cursor, or wait for the final result. A timeout means the task is still running, not that it failed. Cancellation is cooperative and does not undo completed writes/downloads.

```json
{
  "jsonrpc": "2.0",
  "id": 20,
  "method": "tasks.get",
  "params": {
    "task_id": "<returned-task-id>",
    "after": 0
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "id": 21,
  "method": "tasks.result",
  "params": {
    "task_id": "<returned-task-id>",
    "wait_ms": 30000
  }
}
```

tasks.result returns the installed registry entry; then read tools.get_settings/tools.files if you need to configure it. A failed/cancelled task is not a successful installation confirmation.

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32602` / `invalid_params` | `name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The tool folder is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |
| `-32010` / `tool_operation_failed` | The installer, registry or tool itself failed (its message is passed on) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |
| `-32008` / `cancelled` | `tasks.cancel` stopped it before or after the download (from the task's result) | Treat as cancelled; review partial side effects before starting new work |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tools.install",
  "params": {
    "kind": "skill",
    "name": "registry-tool",
    "__unknown_option": true
  }
}
```

The error envelope is `{"jsonrpc":"2.0","id":90,"error":{"code":-32602,"message":"...","data":{"type":"invalid_params", "errors":[...]}}}`. Exact message and field diagnostics are generated by typed deserialization; callers should use codes and structured data, not match a sentence.

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Python

Install `requests` in your client environment. This executes the basic request; edit `params` for a different valid case.

```python
import json
import os
import requests

payload = json.loads(r'''{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tools.install",
  "params": {
    "kind": "skill",
    "name": "registry-tool"
  }
}''')
response = requests.post(
    "http://127.0.0.1:8000/v1/cep",
    headers={"Authorization": "Bearer " + os.environ["CLUAIZ_TOKEN"]},
    json=payload, timeout=60,
)
response.raise_for_status()
reply = response.json()
if "error" in reply:
    raise RuntimeError(json.dumps(reply["error"], ensure_ascii=False))
print(reply["result"])
```

## JavaScript / Node.js

Use an owner token only in a trusted client. Do not ship it inside a public web page.

```javascript
const payload = {
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tools.install",
  "params": {
    "kind": "skill",
    "name": "registry-tool"
  }
};
const response = await fetch("http://127.0.0.1:8000/v1/cep", {
  method: "POST",
  headers: { "content-type": "application/json", authorization: `Bearer ${process.env.CLUAIZ_TOKEN}` },
  body: JSON.stringify(payload),
});
const raw = await response.text();
if (!response.ok) throw new Error(`HTTP ${response.status}: ${raw}`);
const reply = JSON.parse(raw);
if (reply.error) throw new Error(JSON.stringify(reply.error));
console.log(reply.result);
```

## Limits, state and related operations

A cancellation is checked before and after the installer call, not guaranteed to interrupt every native/download step instantly. A package can already exist by the time cancellation is observed; inspect tools.list. Names refer to registry tools, not Hugging Face model repos. Package locations come from the existing registry/installer and pass caller roots. Relative file operations stay inside that package even for the unrestricted owner; absolute paths, .. and escaping symlinks are not accepted as assets.

- [`tasks.get`](../../system/tasks/tasks.get.md): use its separate documented contract.
- [`tasks.result`](../../system/tasks/tasks.result.md): use its separate documented contract.
- [`tools.list`](tools.list.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tools/lifecycle.rs](../../../../../api/src/cep/methods/tools/lifecycle.rs)
- [api/src/cep/methods/tools/common.rs](../../../../../api/src/cep/methods/tools/common.rs)
