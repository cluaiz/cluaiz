---
method: "fs.mkdir"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Creates a directory and missing parents"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "path"
    type: "string"
    req: true
    desc: "Absolute, or relative to the workspace"
    example: "documents"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.mkdir",
    "params": {
      "path": "documents"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "/project/documents"
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "An empty, drive-relative or root-relative path"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.mkdir

Creates a folder and every missing parent. An existing folder is not an error.

## How a path is resolved

1. A relative path is joined to the Engine's workspace ([`workspace.get`](../workspace/workspace.get.md)), never to the Engine's own working folder. `C:foo` (drive-relative) and `\foo` (root-relative) are refused as ambiguous.
2. The path is canonicalized: links and `..` are resolved, so the check below sees the real target.
3. The canonical path must be inside the caller's roots. The owner (`/v1/cep`, the C library) has no roots and may use any path; the MCP principal may only read.

The answered paths are canonical and absolute, without the Windows `\\?\` prefix.

Answers the canonical `path`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.mkdir","params":{"path":"documents/2026"}}'
```

Leave out the `Authorization` header when API authentication is off.
