---
method: "fs.list"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Lists directory entries, optionally recursively"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "path"
    type: "string | null"
    req: false
    desc: "The folder to list (default: `root`, else the workspace)"
    example: "."
  - name: "recursive"
    type: "boolean"
    req: false
    desc: "Also list sub-folders, at most 12 levels deep"
    example: false
  - name: "root"
    type: "string | null"
    req: false
    desc: "The folder the answered `path` values are relative to (default: the workspace)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.list",
    "params": {
      "path": ".",
      "recursive": false
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "items": [
        {
          "is_folder": false,
          "modified": "2026-10-25T00:00:00.000Z",
          "name": "notes.txt",
          "path": "notes.txt",
          "size": 5
        }
      ],
      "skipped": 0,
      "truncated": false
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

# fs.list

Lists the entries of a folder, optionally with all sub-folders.

## How a path is resolved

1. A relative path is joined to the Engine's workspace ([`workspace.get`](../workspace/workspace.get.md)), never to the Engine's own working folder. `C:foo` (drive-relative) and `\foo` (root-relative) are refused as ambiguous.
2. The path is canonicalized: links and `..` are resolved, so the check below sees the real target.
3. The canonical path must be inside the caller's roots. The owner (`/v1/cep`, the C library) has no roots and may use any path; the MCP principal may only read.

The answered paths are canonical and absolute, without the Windows `\\?\` prefix.

## What it does

1. Resolves `path` (default: `root`, else the workspace) and `root` (default: the workspace). The answered `path` of each entry is relative to `root`.
2. Reads the folder. With `recursive: true` it descends into sub-folders, at most 12 levels, and never into a folder link it already visited.
3. An entry it cannot read, or one outside the caller's roots, is skipped and counted instead of failing the whole listing.

## Result

| Field | Meaning |
|---|---|
| `items[]` | `path` (relative to `root`), `name`, `is_folder`, `size` (0 for folders), `modified` (RFC 3339, UTC) |
| `truncated` | `true` when something was skipped or the depth limit was reached |
| `skipped` | How many entries or folders were skipped |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.list","params":{"path":"src","recursive":true}}'
```

Leave out the `Authorization` header when API authentication is off.
