---
method: "fs.copy"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Copies a file or directory between authorized paths"
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
    example: "notes.txt"
  - name: "to"
    type: "string"
    req: true
    desc: "The new path: absolute, or relative to the workspace; missing parent folders are created"
    example: "copy.txt"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.copy",
    "params": {
      "from": "notes.txt",
      "to": "copy.txt"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "from": "/project/notes.txt",
      "to": "/project/copy.txt"
    }
  }
errors:
  - code: -32004
    type: "file_not_found"
    when: "`from` does not exist"
  - code: -32602
    type: "invalid_params"
    when: "`to` is inside `from`, or the copied folder holds a link cycle"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.copy

Copies a file, or a folder with everything in it.

## How a path is resolved

1. A relative path is joined to the Engine's workspace ([`workspace.get`](../workspace/workspace.get.md)), never to the Engine's own working folder. `C:foo` (drive-relative) and `\foo` (root-relative) are refused as ambiguous.
2. The path is canonicalized: links and `..` are resolved, so the check below sees the real target.
3. The canonical path must be inside the caller's roots. The owner (`/v1/cep`, the C library) has no roots and may use any path; the MCP principal may only read.

The answered paths are canonical and absolute, without the Windows `\\?\` prefix.

## What it does

1. Resolves `from` (must exist) and `to` (missing parents are created); both are checked against the caller's roots once.
2. Refuses a copy into itself (`to` inside `from`).
3. Copies. For a folder, each entry is copied in turn; an entry whose real path leaves `from`, or whose copy would leave `to`, stops the copy with `-32002`, and a link cycle stops it with `-32602`.

Answers `from` and `to` as canonical paths.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.copy","params":{"from":"templates/base","to":"projects/new"}}'
```

Leave out the `Authorization` header when API authentication is off.
