---
method: "approvals.list"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "approvals"
summary: "Approval requests waiting for the owner"
scope: "admin"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "approvals.list",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "pending": [
        {
          "created_at": "2026-10-25T10:30:00Z",
          "expires_at": "2026-10-25T10:35:00Z",
          "id": "ap_41c2…",
          "method": "mcp:web-search",
          "principal": "agent",
          "status": "pending",
          "target": "{\"query\":\"news\"}"
        }
      ]
    }
  }
errors: []
---

# approvals.list

The approval requests waiting for the owner's answer, oldest first.

An approval request comes from one of three places:

- a tool call the chat agent wants to make (the stream shows it as `permission_request` with the same id);
- a [`system.exec`](../../system/system/system.exec.md) call;
- a CEP call outside its caller's roots, for a principal that asks instead of refusing.

Each request is bound to the caller, the method and a hash of the exact params, waits at most 5 minutes, and is used once.

## Entry fields

`id`, `principal` (who asks: `owner`, `agent`…), `method` (for a chat tool: `mcp:<tool>` / `plugin:<tool>`), `target` (what it would touch: a path, the arguments), `created_at`, `expires_at`, `status` (`pending`).

Only the owner may call it.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"approvals.list","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
