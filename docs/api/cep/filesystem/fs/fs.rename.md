---
method: "fs.rename"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Moves a file or directory between authorized paths; a link is moved itself"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "from"
    type: "string"
    req: true
    desc: "The existing file or folder: absolute, or relative to the workspace"
    example: "old.txt"
  - name: "to"
    type: "string"
    req: true
    desc: "The new path: absolute, or relative to the workspace; missing parent folders are created"
    example: "new.txt"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.rename",
    "params": {
      "from": "old.txt",
      "to": "new.txt"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "from": "/project/old.txt",
      "to": "/project/new.txt"
    }
  }
errors:
  - code: -32004
    type: "file_not_found"
    when: "`from` does not exist"
  - code: -32602
    type: "invalid_params"
    when: "`from` does not end in a file or folder name, or a path is drive-relative / root-relative"
  - code: -32005
    type: "file_exists"
    when: "The OS refused because `to` exists and cannot be replaced"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.rename

Move or rename a filesystem entry between authorized locations. This changes the selected link entry itself when the source is a symlink; it does not intentionally rename its target.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `fs.rename` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Resolve/authorize the source entry and destination
2. Create missing destination parents
3. Ask the OS to rename/move and return the two resolved paths

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Resolve/authorize the source entry and destination"]
    P0 --> P1["Create missing destination parents"]
    P1 --> P2["Ask the OS to rename/move and return the two resolved paths"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `from` | string | Yes | The existing file or folder: absolute, or relative to the workspace Paths are on the Engine host, not the browser. Absolute paths or paths relative to the selected workspace are accepted after canonicalization and caller-root checks. Empty, drive-relative and root-relative ambiguous paths are invalid. Existing entry required; final symlink is treated as the selected entry. |
| `to` | string | Yes | The new path: absolute, or relative to the workspace; missing parent folders are created Paths are on the Engine host, not the browser. Absolute paths or paths relative to the selected workspace are accepted after canonicalization and caller-root checks. Empty, drive-relative and root-relative ambiguous paths are invalid. Missing destination parents are created. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "fs.rename",
  "params": {
    "from": "old.txt",
    "to": "new.txt"
  }
}
```

### 2. Move into a new parent

Creates archive if needed, then attempts the move.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "fs.rename",
  "params": {
    "from": "notes.txt",
    "to": "archive/notes.txt"
  }
}
```

## Response and field meanings

from and to are resolved host paths. The method does not return copied bytes or stream progress. A successful rename is not a model/package registry update.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "from": "/project/old.txt",
    "to": "/project/new.txt"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `file_not_found` | `from` does not exist | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | `from` does not end in a file or folder name, or a path is drive-relative / root-relative | Correct the option/value combination; install required assets where relevant |
| `-32005` / `file_exists` | The OS refused because `to` exists and cannot be replaced | Inspect current state before retrying; avoid a tight retry loop |
| `-32002` / `forbidden` | The path is outside the caller's roots (`data.needs: "roots"`), or the OS refused access (`os_permission_denied`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "fs.rename",
  "params": {
    "from": "old.txt",
    "to": "new.txt",
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
  "method": "fs.rename",
  "params": {
    "from": "old.txt",
    "to": "new.txt"
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
  "method": "fs.rename",
  "params": {
    "from": "old.txt",
    "to": "new.txt"
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

Cross-volume moves and replacement of existing targets depend on OS rename semantics; there is no fallback copy-delete transaction. A file_exists conflict or filesystem error must be handled explicitly. Source parent creation is not undone if the move fails.

- [`fs.copy`](fs.copy.md): use its separate documented contract.
- [`fs.list`](fs.list.md): use its separate documented contract.
- [`fs.mkdir`](fs.mkdir.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/fs.rs](../../../../../api/src/cep/methods/fs.rs)
