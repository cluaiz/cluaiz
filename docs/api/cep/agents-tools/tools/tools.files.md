---
method: "tools.files"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Files inside a tool package, with cache counts"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "skill"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "installed-tool"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.files",
    "params": {
      "kind": "skill",
      "name": "installed-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cache": {
        "all_bytes": 0,
        "entries": [],
        "temp_bytes": 0
      },
      "files": [
        {
          "is_dir": false,
          "name": "SKILL.md",
          "path": "SKILL.md"
        },
        {
          "is_dir": true,
          "name": "assets",
          "path": "assets"
        }
      ],
      "skipped": 0,
      "truncated": false
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
---

# tools.files

The files inside a tool's folder (paths relative to it), with its cache usage. At most 10 000 entries are listed; `truncated` and `skipped` say when more were left out.

## Result

| Field | Meaning |
|---|---|
| `files[]` | `path` (relative), `name`, `is_dir` |
| `cache` | `entries`, `all_bytes`, `temp_bytes` (as in [`tools.cache`](tools.cache.md)) |
| `truncated`, `skipped` | Whether, and how many, entries were left out |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.files","params":{"kind":"skill","name":"code-interpreter"}}'
```

Leave out the `Authorization` header when API authentication is off.
