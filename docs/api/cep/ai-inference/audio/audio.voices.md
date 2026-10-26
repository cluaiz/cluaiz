---
method: "audio.voices"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "audio"
summary: "Voice inventory and speech capabilities without loading weights"
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
    example: "kokoro-82m-onnx"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "audio.voices",
    "params": {
      "model": "kokoro-82m-onnx"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "capabilities": {
        "coherent_input": false,
        "language_control": "explicit",
        "languages": [
          "en-US",
          "en-GB",
          "hi"
        ],
        "voice_mode": "preset"
      },
      "data": [
        {
          "default": true,
          "id": "default",
          "language": null
        },
        {
          "default": false,
          "id": "af_heart",
          "language": "en-US"
        },
        {
          "default": false,
          "id": "hf_alpha",
          "language": "hi"
        }
      ],
      "model": "kokoro-82m-onnx",
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

# audio.voices

Inspect a compatible installed speech package’s voice inventory and capabilities without loading its inference weights. Preset ids, reference-audio conditioning and language controls are model/package-specific.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `audio.voices` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Select the installed TTS entry through the shared audio owner
2. Authorize package paths and inspect the family/package contract
3. Return default/preset voice choices and declared speech capabilities

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Select the installed TTS entry through the shared audio owner"]
    P0 --> P1["Authorize package paths and inspect the family/package contract"]
    P1 --> P2["Return default/preset voice choices and declared speech capabilities"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `model` | string \| null | No | An installed model id (default: the active slot, else the first installed model for the task) Omitted: active compatible TTS slot, otherwise the shared eligible installed selection. Present: exact installed id; not a Hugging Face download request. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "audio.voices",
  "params": {
    "model": "kokoro-82m-onnx"
  }
}
```

### 2. Inspect the active/default TTS package

Uses existing task-aware selection.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "audio.voices",
  "params": {}
}
```

### 3. Inspect a chosen installed package

Use this inventory when building the corresponding speech request.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "audio.voices",
  "params": {
    "model": "<installed-tts-id>"
  }
}
```

## Response and field meanings

object:"list" and model identify the inventory. data[] voice ids/languages include a default choice. capabilities describes languages, voice mode, language control and related constraints. A default-only GGUF inventory can mean reference-audio mode rather than dozens of missing presets.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "capabilities": {
      "coherent_input": false,
      "language_control": "explicit",
      "languages": [
        "en-US",
        "en-GB",
        "hi"
      ],
      "voice_mode": "preset"
    },
    "data": [
      {
        "default": true,
        "id": "default",
        "language": null
      },
      {
        "default": false,
        "id": "af_heart",
        "language": "en-US"
      },
      {
        "default": false,
        "id": "hf_alpha",
        "language": "hi"
      }
    ],
    "model": "kokoro-82m-onnx",
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
  "method": "audio.voices",
  "params": {
    "model": "kokoro-82m-onnx",
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
  "method": "audio.voices",
  "params": {
    "model": "kokoro-82m-onnx"
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
  "method": "audio.voices",
  "params": {
    "model": "kokoro-82m-onnx"
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

The call inspects package assets, not actual audible synthesis, so it does not certify latency/quality. A valid registry declaration without required voice/config assets still returns a contract error. Do not reuse one package’s voice id blindly in another.

- [`audio.run`](audio.run.md): use its separate documented contract.
- [`models.list`](../../models/models/models.list.md): use its separate documented contract.
- [`audio.languages`](audio.languages.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/audio.rs](../../../../../api/src/cep/methods/audio.rs)
