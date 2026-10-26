---
method: "fs.write"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Writes exactly one of text or data_base64 to a file"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "data_base64"
    type: "string | null"
    req: false
    desc: "The file's content as standard base64 (send this or `text`)"
    example: null
  - name: "path"
    type: "string"
    req: true
    desc: "Absolute, or relative to the workspace; missing parent folders are created"
    example: "notes.txt"
  - name: "text"
    type: "string | null"
    req: false
    desc: "The file's content as UTF-8 text (send this or `data_base64`)"
    example: "hello"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.write",
    "params": {
      "path": "notes.txt",
      "text": "hello"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "bytes": 5,
      "path": "/project/notes.txt"
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "Both or neither of `text` / `data_base64`, invalid base64, or a bad path"
  - code: -32002
    type: "forbidden"
    when: "The path is outside the caller's roots (`data.needs: \"roots\"`), or the OS refused access (`os_permission_denied`)"
---

# fs.write

Writes a file, replacing it when it exists. Send the content as `text` (UTF-8) or as `data_base64`, never both. Missing parent folders are created.

## How a path is resolved

1. A relative path is joined to the Engine's workspace ([`workspace.get`](../workspace/workspace.get.md)), never to the Engine's own working folder. `C:foo` (drive-relative) and `\foo` (root-relative) are refused as ambiguous.
2. The path is canonicalized: links and `..` are resolved, so the check below sees the real target.
3. The canonical path must be inside the caller's roots. The owner (`/v1/cep`, the C library) has no roots and may use any path; the MCP principal may only read.

The answered paths are canonical and absolute, without the Windows `\\?\` prefix.

For a new file, the closest existing parent folder is canonicalized and the missing part is added to it, so `new/../../outside` cannot pass as a path inside the workspace. A dangling link in the path is refused.

## Result

`path` (canonical) and `bytes` written.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.write","params":{"path":"notes/todo.txt","text":"Buy milk"}}'
```

Leave out the `Authorization` header when API authentication is off.
