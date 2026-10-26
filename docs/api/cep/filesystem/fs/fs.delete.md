---
method: "fs.delete"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Deletes a file or directory; a link is removed itself, never its target"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params:
  - name: "path"
    type: "string"
    req: true
    desc: "Absolute, or relative to the workspace"
    example: "old.txt"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.delete",
    "params": {
      "path": "old.txt"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "/project/old.txt"
    }
  }
errors:
  - code: -32004
    type: "file_not_found"
    when: "The path does not exist"
  - code: -32602
    type: "invalid_params"
    when: "The path does not end in a file or folder name (for example `..`)"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.delete

Delete a single selected file/directory entry. For a directory this removes its tree; for a symbolic link it removes the link itself rather than recursively deleting its target.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `fs.delete` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Destructive; review the target before calling |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Resolve the parent and authorize the selected entry without dereferencing its final link
2. Inspect entry type with symlink metadata
3. Remove the link/file/directory and return the selected resolved path

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Resolve the parent and authorize the selected entry without dereferencing its final link"]
    P0 --> P1["Inspect entry type with symlink metadata"]
    P1 --> P2["Remove the link/file/directory and return the selected resolved path"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `path` | string | Yes | Absolute, or relative to the workspace Paths are on the Engine host, not the browser. Absolute paths or paths relative to the selected workspace are accepted after canonicalization and caller-root checks. Empty, drive-relative and root-relative ambiguous paths are invalid. Must end in a concrete file/folder entry name, not .. or an unnamed root. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "fs.delete",
  "params": {
    "path": "old.txt"
  }
}
```

### 2. Remove a disposable directory tree

Every child of the selected real directory is targeted; choose disposable data.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "fs.delete",
  "params": {
    "path": "generated/disposable-output"
  }
}
```

### 3. Remove only a link

When this is a symbolic link, its target is left intact.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "fs.delete",
  "params": {
    "path": "generated/output-link"
  }
}
```

## Response and field meanings

result.path identifies the deleted entry. There is no deleted:true count, recycle-bin handle or undo token. A nonexistent entry returns file_not_found rather than a success-shaped no-op.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "path": "/project/old.txt"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `file_not_found` | The path does not exist | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | The path does not end in a file or folder name (for example `..`) | Correct the option/value combination; install required assets where relevant |
| `-32002` / `forbidden` | The path is outside the caller's roots (`data.needs: "roots"`), or the OS refused access (`os_permission_denied`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "fs.delete",
  "params": {
    "path": "old.txt",
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
  "method": "fs.delete",
  "params": {
    "path": "old.txt"
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
  "method": "fs.delete",
  "params": {
    "path": "old.txt"
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

Deletion is permanent through the filesystem API. Open/locked files and OS ACLs can prevent it. Do not infer a caller-root denial and an OS access-denied failure are interchangeable. This call has no recursive:false, dry_run or recycle option.

- [`fs.list`](fs.list.md): use its separate documented contract.
- [`fs.copy`](fs.copy.md): use its separate documented contract.
- [`fs.rename`](fs.rename.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/fs.rs](../../../../../api/src/cep/methods/fs.rs)
