---
method: "chat.telemetry"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "How a chat's context is used by its tools, for a model"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "The chat whose attached tools are counted (default: \"default\")"
    example: "c1"
  - name: "model"
    type: "string | null"
    req: false
    desc: "The model whose context window is measured (default: the active chat model, \"default\")"
    example: "qwen3-4b-gguf-Q4_K_M"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "chat.telemetry",
    "params": {
      "chat_id": "c1",
      "model": "qwen3-4b-gguf-Q4_K_M"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "model": "qwen3-4b-gguf-Q4_K_M",
      "telemetry": {}
    }
  }
errors: []
---

# chat.telemetry

Inspect the existing ToolsEngine context/tool telemetry projection for a model and conversation. It is a planning aid, not a new generation, a full memory event log or a GPU performance benchmark.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `chat.telemetry` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Resolve omitted model/chat selectors to the literal default selection used by ToolsEngine
2. Read the active tool ids of that chat
3. Compute tool telemetry and return it with selection identity

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Resolve omitted model/chat selectors to the literal default selection used by ToolsEngine"]
    P0 --> P1["Read the active tool ids of that chat"]
    P1 --> P2["Compute tool telemetry and return it with selection identity"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `chat_id` | string \| null | No | The chat whose attached tools are counted (default: "default") Omitted: the handler uses "default", not an implicit visible browser chat. |
| `model` | string \| null | No | The model whose context window is measured (default: the active chat model, "default") Omitted: the handler passes "default" to the existing telemetry owner. Supply an actual model id for a specific comparison. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "chat.telemetry",
  "params": {
    "chat_id": "c1",
    "model": "qwen3-4b-gguf-Q4_K_M"
  }
}
```

### 2. Use the default selectors

Both selectors use the handler’s default values.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "chat.telemetry",
  "params": {}
}
```

### 3. Inspect one session/model

Replace the model id; the call does not create a chat completion.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "chat.telemetry",
  "params": {
    "chat_id": "c1",
    "model": "<installed-chat-model>"
  }
}
```

## Response and field meanings

model and chat_id identify the selection; telemetry is the existing tool/context report. The handler supplies zero for its extra request counters, so this is not the exact measured token usage of a full pending prompt.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "chat_id": "c1",
    "model": "qwen3-4b-gguf-Q4_K_M",
    "telemetry": {}
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "chat.telemetry",
  "params": {
    "chat_id": "c1",
    "model": "qwen3-4b-gguf-Q4_K_M",
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
  "method": "chat.telemetry",
  "params": {
    "chat_id": "c1",
    "model": "qwen3-4b-gguf-Q4_K_M"
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
  "method": "chat.telemetry",
  "params": {
    "chat_id": "c1",
    "model": "qwen3-4b-gguf-Q4_K_M"
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

No model weights are generated through this method. Use context.preview for full shared preflight with messages and system.plan for load planning. Different selectors can change the report without changing saved settings.

- [`context.preview`](../context/context.preview.md): use its separate documented contract.
- [`system.plan`](../../system/system/system.plan.md): use its separate documented contract.
- [`chat.tools`](chat.tools.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/chat/mod.rs](../../../../../api/src/cep/methods/chat/mod.rs)
