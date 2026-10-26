---
method: "models.remove"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Unloads the selected model if active and removes its installed package"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params:
  - name: "model"
    type: "string"
    req: true
    desc: "An installed model id (from `models.list`)"
    example: "installed-model"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.remove",
    "params": {
      "model": "installed-model"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "dir": "models/chat/installed-model",
      "model": "installed-model"
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or its weight file is missing"
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty or looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "model_remove_failed"
    when: "The folder or the registry entry could not be removed"
---

# models.remove

Uninstall a registered model package, including unregistering it. Use this to reclaim an installed package deliberately; it is not only an unload or deletion of a single quantization file.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `models.remove` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Destructive; review the target before calling |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Select and authorize the registered package directory
2. Unload the shared host if its active model path belongs to that directory
3. Delegate uninstall/registry removal to the existing ModelDownloader

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Select and authorize the registered package directory"]
    P0 --> P1["Unload the shared host if its active model path belongs to that directory"]
    P1 --> P2["Delegate uninstall/registry removal to the existing ModelDownloader"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `model` | string | Yes | An installed model id (from `models.list`) Exact installed id; the handler does not accept an arbitrary directory as the target. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "models.remove",
  "params": {
    "model": "installed-model"
  }
}
```

### 2. Uninstall a selected package

Confirm that this is the intended installed id and that no active work needs its assets.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "models.remove",
  "params": {
    "model": "<installed-id-to-remove>"
  }
}
```

## Response and field meanings

model is the removed registry id; dir reports the uninstall owner’s resolved directory. The operation does not return an undo token or a restored package manifest. Refresh installed-model selectors after removal.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "dir": "models/chat/installed-model",
    "model": "installed-model"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `model_not_found` | No installed model with this id, or its weight file is missing | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | `model` is empty or looks like a path | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The model's folder is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |
| `-32010` / `model_remove_failed` | The folder or the registry entry could not be removed | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "models.remove",
  "params": {
    "model": "installed-model",
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
  "method": "models.remove",
  "params": {
    "model": "installed-model"
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
  "method": "models.remove",
  "params": {
    "model": "installed-model"
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

This is destructive and can fail on locked files/OS permissions. The handler explicitly checks the shared host’s active model path; do not infer that it gracefully terminates every task-specific ONNX/speech resident or external client. Stop relevant use before uninstalling. A failed removal is not a transaction/rollback promise.

- [`models.list`](models.list.md): use its separate documented contract.
- [`models.load`](models.load.md): use its separate documented contract.
- [`models.download`](models.download.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/models.rs](../../../../../api/src/cep/methods/models.rs)
