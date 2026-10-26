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

Deletes a file, or a folder with everything in it. A link is deleted itself; the file or folder it points to is not touched.

## How the path is resolved

The parent folder is canonicalized and the last name is kept as it is (what `rm` does), so `fs.delete {"path": "link-to-docs"}` removes the link `link-to-docs` and leaves `docs` alone. The resulting path must exist and be inside the caller's roots. A path that ends in `..` or `.` without a name is refused.

## What it does

- A link: removed (a link to a folder is removed as a folder entry on Windows).
- A folder: removed with its contents. Links inside it are removed, never followed.
- A file: removed.

Answers the `path` it removed.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.delete","params":{"path":"old/report.txt"}}'
```

Leave out the `Authorization` header when API authentication is off.
