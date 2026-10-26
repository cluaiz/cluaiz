---
method: "storage.clean"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "storage"
summary: "Removes cached temporary media"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "storage.clean",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "freed_bytes": 0,
      "removed_files": 0
    }
  }
errors:
  - code: -32005
    type: "storage_path_invalid"
    when: "The temporary media path exists but is not a folder"
  - code: -32002
    type: "forbidden"
    when: "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)"
  - code: -32010
    type: "filesystem_error"
    when: "A file could not be removed (for example, another program holds it open)"
---

# storage.clean

Removes everything in the temporary media folder and answers how much it freed.

## What it does

1. Counts the folder like [`storage.status`](storage.status.md); a link or a file in place of the folder fails the call before anything is removed.
2. Removes the folder with its contents (links inside are removed, their targets are not touched) and creates it again empty.
3. Answers `removed_files` and `freed_bytes`.

A file another program holds open makes the call fail with `-32010`; files removed before it stay removed.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"storage.clean","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
