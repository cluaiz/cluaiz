---
method: "fs.read"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Reads a UTF-8 file, or returns base64 for binary data"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "binary"
    type: "boolean"
    req: false
    desc: "Always answer `data_base64`, even for UTF-8 text"
    example: null
  - name: "path"
    type: "string"
    req: true
    desc: "Absolute, or relative to the workspace"
    example: "notes.txt"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.read",
    "params": {
      "path": "notes.txt"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "binary": false,
      "path": "/project/notes.txt",
      "size": 5,
      "text": "hello"
    }
  }
errors:
  - code: -32004
    type: "file_not_found"
    when: "The path does not exist"
  - code: -32602
    type: "invalid_params"
    when: "An empty, drive-relative or root-relative path"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.read

Reads a whole file. UTF-8 text comes back as `text`; anything else (or every file when `binary: true`) comes back as standard base64 in `data_base64`.

## How a path is resolved

1. A relative path is joined to the Engine's workspace ([`workspace.get`](../workspace/workspace.get.md)), never to the Engine's own working folder. `C:foo` (drive-relative) and `\foo` (root-relative) are refused as ambiguous.
2. The path is canonicalized: links and `..` are resolved, so the check below sees the real target.
3. The canonical path must be inside the caller's roots. The owner (`/v1/cep`, the C library) has no roots and may use any path; the MCP principal may only read.

The answered paths are canonical and absolute, without the Windows `\\?\` prefix.

## Result

| Field | Meaning |
|---|---|
| `path` | The canonical path that was read |
| `size` | Bytes in the file |
| `binary` | `false` with `text`, `true` with `data_base64` |
| `text` / `data_base64` | The content |

The whole file is read into memory; there is no range read. For large binary files, prefer reading them where they are (chat attachments and `ingest.run` take paths).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.read","params":{"path":"notes.txt"}}'
```

Leave out the `Authorization` header when API authentication is off.
