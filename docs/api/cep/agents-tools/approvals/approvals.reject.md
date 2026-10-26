---
method: "approvals.reject"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "approvals"
summary: "Refuses the waiting call"
scope: "admin"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: false
params:
  - name: "approval_id"
    type: "string"
    req: true
    desc: "The `id` of the pending approval"
    example: "ap_41c2…"
  - name: "feedback"
    type: "string | null"
    req: false
    desc: "Passed to the waiting call (the model sees it as the reason)"
    example: "Not this site"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "approvals.reject",
    "params": {
      "approval_id": "ap_41c2…",
      "feedback": "Not this site"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "approval": {
        "created_at": "2026-10-25T10:30:00Z",
        "expires_at": "2026-10-25T10:35:00Z",
        "id": "ap_41c2…",
        "method": "mcp:web-search",
        "principal": "agent",
        "status": "pending",
        "target": "{\"query\":\"news\"}"
      }
    }
  }
errors:
  - code: -32004
    type: "approval_not_found"
    when: "No pending approval with this id, or it expired (5 minutes)"
  - code: -32005
    type: "approval_decided"
    when: "The approval was already approved or rejected"
---

# approvals.reject

Refuses one waiting request. A chat tool call does not run; the agent gets `feedback` as the reason and continues its answer without the tool. A CEP call sent again with this id is refused.

Answers the approval (`status: "rejected"`). Only the owner may call it.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"approvals.reject","params":{"approval_id":"ap_41c2…","feedback":"Not this site"}}'
```

Leave out the `Authorization` header when API authentication is off.
