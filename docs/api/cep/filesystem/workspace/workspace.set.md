---
method: "workspace.set"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "workspace"
summary: "Changes the workspace to an existing directory"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "path"
    type: "string"
    req: true
    desc: "An existing folder: absolute, or relative to the current workspace"
    example: "/project"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "workspace.set",
    "params": {
      "path": "/project"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "/project"
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "The path is not a folder, or is drive-relative / root-relative"
  - code: -32004
    type: "file_not_found"
    when: "The folder does not exist"
  - code: -32002
    type: "forbidden"
    when: "The folder is outside the caller's roots (`data.needs: \"roots\"`)"
---

# workspace.set

Change the shared Engine workspace to an existing directory. Both the AppState and existing ToolsEngine workspace owner receive the same selected folder so later relative-path consumers agree.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `workspace.set` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Resolve path relative to the current workspace
2. Require an existing directory and authorize it
3. Update the existing shared workspace owners

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Resolve path relative to the current workspace"]
    P0 --> P1["Require an existing directory and authorize it"]
    P1 --> P2["Update the existing shared workspace owners"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `path` | string | Yes | An existing folder: absolute, or relative to the current workspace Paths are on the Engine host, not the browser. Absolute paths or paths relative to the selected workspace are accepted after canonicalization and caller-root checks. Empty, drive-relative and root-relative ambiguous paths are invalid. Must already be a directory; this call does not create it. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "workspace.set",
  "params": {
    "path": "/project"
  }
}
```

### 2. Select a sibling project

The canonical target must exist and be authorized; .. itself is not a permission bypass.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "workspace.set",
  "params": {
    "path": "../another-project"
  }
}
```

## Response and field meanings

path is the new resolved workspace. The response is a selected location, not a directory snapshot or a new permission root.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "path": "/project"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32602` / `invalid_params` | The path is not a folder, or is drive-relative / root-relative | Correct the option/value combination; install required assets where relevant |
| `-32004` / `file_not_found` | The folder does not exist | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32002` / `forbidden` | The folder is outside the caller's roots (`data.needs: "roots"`) | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "workspace.set",
  "params": {
    "path": "/project",
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
  "method": "workspace.set",
  "params": {
    "path": "/project"
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
  "method": "workspace.set",
  "params": {
    "path": "/project"
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

No file contents are moved. Existing model/session state is not reset, and a principal does not gain access to arbitrary targets by changing workspace. This is not advertised through the current MCP facade. Avoid racing workspace changes with assumptions about other clients’ relative paths.

- [`workspace.get`](workspace.get.md): use its separate documented contract.
- [`fs.mkdir`](../fs/fs.mkdir.md): use its separate documented contract.
- [`fs.list`](../fs/fs.list.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/workspace.rs](../../../../../api/src/cep/methods/workspace.rs)
