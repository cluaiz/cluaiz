---
method: "tools.cache"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Actual cache entries and byte counts for installed tools"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "skill"
  - name: "name"
    type: "string | null"
    req: false
    desc: "One tool; every tool of `kind` when absent"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.cache",
    "params": {
      "kind": "skill"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": [
      {
        "all_bytes": 0,
        "entries": [],
        "kind": "skill",
        "name": "code-interpreter",
        "temp_bytes": 0
      }
    ]
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
---

# tools.cache

Read actual cache-file statistics for one installed tool or all installed tools of a kind. This is package .cache storage, separate from temp_media and saved chat KV.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tools.cache` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Choose a single target or authorize the shared category root
2. Validate every selected installed package location
3. Scan recognized direct cache files and return per-package rows

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Choose a single target or authorize the shared category root"]
    P0 --> P1["Validate every selected installed package location"]
    P1 --> P2["Scan recognized direct cache files and return per-package rows"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `kind` | "skill" \| "plugin" \| "mcp" | Yes | skill, plugin or mcp Required enum: skill (instructions), plugin (WASM/native package), mcp (server package). Kind and name must identify the same installed entry. |
| `name` | string \| null | No | One tool; every tool of `kind` when absent Optional valid installed name; omitted: every installed package of the chosen kind. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools.cache",
  "params": {
    "kind": "skill"
  }
}
```

### 2. Read one package cache

Use a real installed tool. A missing cache is a valid empty result.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools.cache",
  "params": {
    "kind": "skill",
    "name": "installed-tool"
  }
}
```

## Response and field meanings

result is an array. Each row identifies kind/name plus all_bytes, temp_bytes and entries[{path,size}]. temp_bytes recognizes temp_ names or .tmp endings. Missing .cache reports zero; direct-file scans do not imply recursive total package size.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": [
    {
      "all_bytes": 0,
      "entries": [],
      "kind": "skill",
      "name": "code-interpreter",
      "temp_bytes": 0
    }
  ]
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `tool_not_found` | No installed tool of this kind and name | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | `name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The tool folder is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tools.cache",
  "params": {
    "kind": "skill",
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
  "method": "tools.cache",
  "params": {
    "kind": "skill"
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
  "method": "tools.cache",
  "params": {
    "kind": "skill"
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

This does not delete files and does not report all dependency/weight storage. A cache path escaping its package is rejected. File counts can change while tools are running. Package locations come from the existing registry/installer and pass caller roots. Relative file operations stay inside that package even for the unrestricted owner; absolute paths, .. and escaping symlinks are not accepted as assets.

- [`tools.clear_cache`](tools.clear_cache.md): use its separate documented contract.
- [`tools.files`](tools.files.md): use its separate documented contract.
- [`storage.status`](../../system/storage/storage.status.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tools/lifecycle.rs](../../../../../api/src/cep/methods/tools/lifecycle.rs)
- [api/src/cep/methods/tools/files.rs](../../../../../api/src/cep/methods/tools/files.rs)
