---
method: "workspace.get"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "workspace"
summary: "Returns the current workspace directory"
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
    "method": "workspace.get",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "/project"
    }
  }
errors: []
---

# workspace.get

The Engine's current workspace folder: the base of relative paths in `fs.*`, of chat attachments with relative paths, and of the agent's file tools.

Answers the canonical `path`. At start it is the Engine's home folder (`~/.cluaiz` by default, or `CLUAIZ_HOME`), until a client sets another.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"workspace.get","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
