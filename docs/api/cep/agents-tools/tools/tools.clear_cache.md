---
method: "tools.clear_cache"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Remove cached files without uninstalling tools"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: true
idempotent: false
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
    "method": "tools.clear_cache",
    "params": {
      "kind": "skill"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "freed_bytes": 0,
      "removed": 0
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
  - code: -32010
    type: "filesystem_error"
    when: "A cached file could not be removed"
---

# tools.clear_cache

Removes the files in tools' `.cache` folders without uninstalling them. With `name`, one tool; without it, every tool of `kind`.

Every targeted `.cache` folder is checked first (it must be a real folder inside the caller's roots); only then is anything removed. Answers `removed` (files) and `freed_bytes`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.clear_cache","params":{"kind":"skill","name":"code-interpreter"}}'
```

Leave out the `Authorization` header when API authentication is off.
