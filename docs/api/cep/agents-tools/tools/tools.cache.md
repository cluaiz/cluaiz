---
method: "tools.cache"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Actual cache entries and byte counts for installed tools"
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
    type: "string | null"
    req: false
    desc: "One tool; every tool of `kind` when absent"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.cache",
    "params": {
      "kind": "skill"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": [
      {
        "all_bytes": 0,
        "entries": [],
        "kind": "skill",
        "name": "code-interpreter",
        "temp_bytes": 0
      }
    ]
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

# tools.cache

What each tool keeps in its `.cache` folder: the entries and their bytes, counted on disk. With `name`, one tool; without it, every tool of `kind`.

## Entry fields

| Field | Meaning |
|---|---|
| `kind`, `name` | The tool |
| `entries[]` | The files and folders in `.cache` |
| `all_bytes` | Bytes in `.cache` |
| `temp_bytes` | Bytes in files named `temp_*` or `*.tmp` |

A tool without a `.cache` folder answers zeros. A `.cache` that is a link fails the call instead of being followed.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.cache","params":{"kind":"skill"}}'
```

Leave out the `Authorization` header when API authentication is off.
