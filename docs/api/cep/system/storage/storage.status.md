---
method: "storage.status"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "storage"
summary: "Temporary media storage usage"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "storage.status",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "file_count": 0,
      "total_size_bytes": 0,
      "total_size_mb": "0.00 MB"
    }
  }
errors:
  - code: -32005
    type: "storage_path_invalid"
    when: "The temporary media path exists but is not a folder"
  - code: -32002
    type: "forbidden"
    when: "The temporary media folder is a symbolic link (`data.needs: \"roots\"`)"
---

# storage.status

How many files and bytes the temporary media folder holds. Media sent to chat, audio and ingest (downloads, decoded copies) is kept there while it is needed.

## What it does

Walks `temp_media` in the Engine home and counts regular files and their sizes. Links inside it are not followed. A missing folder counts as empty. When `temp_media` itself is a link or a file, the call fails instead of counting somewhere else.

## Result

`file_count`, `total_size_bytes`, and `total_size_mb` as text (`"12.50 MB"`).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"storage.status","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
