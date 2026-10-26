---
method: "storage.status"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "storage"
summary: "Temporary media storage usage"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "storage.status",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "file_count": 0,
      "total_size_bytes": 0,
      "total_size_mb": "0.00 MB"
    }
  }
errors:
  - code: -32005
    type: "storage_path_invalid"
    when: "The temporary media path exists but is not a folder"
  - code: -32002
    type: "forbidden"
    when: "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)"
---

# storage.status

Measure the Engine’s temporary media directory. This is the shared temp_media owner, not model weights, tool caches, saved chat KV or an arbitrary disk path selected by the client.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `storage.status` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Inspect the configured local temp_media location
2. Reject a symbolic-link root or a root that is not a directory
3. Count recognized files/bytes and format the usage response

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Inspect the configured local temp_media location"]
    P0 --> P1["Reject a symbolic-link root or a root that is not a directory"]
    P1 --> P2["Count recognized files/bytes and format the usage response"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `params` | object | No fields | Use `{}`. Unknown fields are rejected; do not invent filters or target identifiers. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "storage.status",
  "params": {}
}
```

## Response and field meanings

file_count and total_size_bytes are numeric counts; total_size_mb is a formatted display string using the Engine MiB constant. A missing directory reports zero usage. Do not parse the display string instead of total_size_bytes.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "file_count": 0,
    "total_size_bytes": 0,
    "total_size_mb": "0.00 MB"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32005` / `storage_path_invalid` | The temporary media path exists but is not a folder | Inspect current state before retrying; avoid a tight retry loop |
| `-32002` / `forbidden` | The temporary media folder is a symbolic link (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "storage.status",
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
  "method": "storage.status",
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
  "method": "storage.status",
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

This is a filesystem snapshot; concurrent work can change contents. The API has no path parameter or global-disk scan. It is not a certification of parser/native allocation bounds. Storage scan/permission failures can still surface through filesystem error handling.

- [`storage.clean`](storage.clean.md): use its separate documented contract.
- [`kv.status`](../../ai-inference/kv/kv.status.md): use its separate documented contract.
- [`tools.cache`](../../agents-tools/tools/tools.cache.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/storage.rs](../../../../../api/src/cep/methods/storage.rs)
