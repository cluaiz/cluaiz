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

Moves or renames a file or folder. A link is moved itself, never its target.

## How the paths are resolved

- `from`: its parent folder is canonicalized and its last name kept (like `mv`), and it must exist.
- `to`: resolved like a new file: missing parent folders are created.
- Both must be inside the caller's roots; one approval covers both when the caller is asked.

The move uses the OS rename, so moving between drives fails with `-32010`; copy then delete instead.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.rename","params":{"from":"draft.txt","to":"final/report.txt"}}'
```

Leave out the `Authorization` header when API authentication is off.
