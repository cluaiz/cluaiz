---
method: "storage.clean"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "storage"
summary: "Removes cached temporary media"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "storage.clean",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "freed_bytes": 0,
      "removed_files": 0
    }
  }
errors:
  - code: -32005
    type: "storage_path_invalid"
    when: "The temporary media path exists but is not a folder"
  - code: -32002
    type: "forbidden"
    when: "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)"
  - code: -32010
    type: "filesystem_error"
    when: "A file could not be removed (for example, another program holds it open)"
---

# storage.clean

Remove the configured temporary media tree and recreate its directory when it existed. Use for deliberate temp-media maintenance, not uninstalling models or clearing every kind of application state.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `storage.clean` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Destructive; review the target before calling |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Scan the fixed temp_media tree and retain its usage report
2. Reject invalid/symlink root conditions before removal
3. Remove/recreate the tree and return reported removed files/freed bytes

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Scan the fixed temp_media tree and retain its usage report"]
    P0 --> P1["Reject invalid/symlink root conditions before removal"]
    P1 --> P2["Remove/recreate the tree and return reported removed files/freed bytes"]
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
  "method": "storage.clean",
  "params": {}
}
```

## Response and field meanings

removed_files/freed_bytes come from the pre-clean scan. Missing temp storage can return zero counts. On filesystem failure the method returns an error instead of inventing successful deletion.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "freed_bytes": 0,
    "removed_files": 0
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
| `-32010` / `filesystem_error` | A file could not be removed (for example, another program holds it open) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "storage.clean",
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
  "method": "storage.clean",
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
  "method": "storage.clean",
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

There is no dry_run, path, age or selected-file option. Concurrent tasks can hold files open or still need temporary data; stop/coordinate relevant work before cleaning. A failure can leave partial effects because this is not transactional deletion. Tool caches and KV use their own APIs.

- [`storage.status`](storage.status.md): use its separate documented contract.
- [`tools.clear_cache`](../../agents-tools/tools/tools.clear_cache.md): use its separate documented contract.
- [`kv.clear`](../../ai-inference/kv/kv.clear.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/storage.rs](../../../../../api/src/cep/methods/storage.rs)
