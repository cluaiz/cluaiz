---
method: "audio.languages"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "audio"
summary: "Languages declared by the selected transcription model"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "model"
    type: "string | null"
    req: false
    desc: "An installed model id (default: the active slot, else the first installed model for the task)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "audio.languages",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "data": [
        "af",
        "ar",
        "bn",
        "de",
        "en",
        "es",
        "fr",
        "hi",
        "ja",
        "zh"
      ],
      "model": "whisper-large-v3-turbo-INT8",
      "object": "list"
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or no installed model for the task"
  - code: -32602
    type: "model_contract_error"
    when: "The model's package does not match its declared task (missing voices, config or assets)"
  - code: -32002
    type: "forbidden"
    when: "The model or the source file is outside the caller's roots (`data.needs: \"roots\"`)"
---

# audio.languages

Read language codes declared by an installed STT package. This is transcription language discovery, not a TTS voice list, a translator catalogue or a model download.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `audio.languages` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Select a compatible installed transcription model
2. Authorize it and validate the ONNX STT contract
3. Return the model’s declared transcription languages

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Select a compatible installed transcription model"]
    P0 --> P1["Authorize it and validate the ONNX STT contract"]
    P1 --> P2["Return the model’s declared transcription languages"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `model` | string \| null | No | An installed model id (default: the active slot, else the first installed model for the task) Omitted: active eligible STT slot or the existing installed fallback. Present: exact installed STT id. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "audio.languages",
  "params": {}
}
```

### 2. Inspect a selected STT package

Copy a real id from models.list; language codes come from this package rather than a hardcoded UI list.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "audio.languages",
  "params": {
    "model": "<installed-stt-id>"
  }
}
```

## Response and field meanings

data is the model’s language code list, with model indicating the selected package. A monolingual package can return an empty list; do not interpret it as universal language support or automatically broken installation.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "data": [
      "af",
      "ar",
      "bn",
      "de",
      "en",
      "es",
      "fr",
      "hi",
      "ja",
      "zh"
    ],
    "model": "whisper-large-v3-turbo-INT8",
    "object": "list"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `model_not_found` | No installed model with this id, or no installed model for the task | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `model_contract_error` | The model's package does not match its declared task (missing voices, config or assets) | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The model or the source file is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "audio.languages",
  "params": {
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
  "method": "audio.languages",
  "params": {}
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
  "method": "audio.languages",
  "params": {}
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

The current path constructs an ONNX STT model contract, not a generic GGUF audio-chat decoder. Language recognition quality is not measured by this inventory request. Supply only supported controls to the subsequent audio request.

- [`audio.run`](audio.run.md): use its separate documented contract.
- [`models.list`](../../models/models/models.list.md): use its separate documented contract.
- [`audio.voices`](audio.voices.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/audio.rs](../../../../../api/src/cep/methods/audio.rs)
