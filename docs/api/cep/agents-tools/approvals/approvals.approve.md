---
method: "approvals.approve"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "approvals"
summary: "Allows the waiting call; with remember chat, the tool also runs without asking in that chat for a while"
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
    desc: "The `id` from `approvals.list`, the `-32003` error's `data.approval_id`, or the chat stream's `permission_request.request_id`"
    example: "ap_41c2…"
  - name: "feedback"
    type: "string | null"
    req: false
    desc: "Passed to the waiting call"
    example: null
  - name: "minutes"
    type: "integer | null"
    req: false
    desc: "How long a \"chat\" grant lasts (default 60, at most 1440)"
    example: 60
  - name: "remember"
    type: "string | null"
    req: false
    desc: "\"chat\": also allow this tool in this chat without asking again, for `minutes`"
    example: "chat"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "approvals.approve",
    "params": {
      "approval_id": "ap_41c2…",
      "minutes": 60,
      "remember": "chat"
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
      },
      "grant": {
        "chat_id": "c1",
        "minutes": 60,
        "tool": "web-search"
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
  - code: -32602
    type: "invalid_params"
    when: "`remember` is something other than \"chat\""
---

# approvals.approve

Allows one waiting request.

An approval request comes from one of three places:

- a tool call the chat agent wants to make (the stream shows it as `permission_request` with the same id);
- a [`system.exec`](../../system/system/system.exec.md) call;
- a CEP call outside its caller's roots, for a principal that asks instead of refusing.

Each request is bound to the caller, the method and a hash of the exact params, waits at most 5 minutes, and is used once.

## What happens next

- A chat tool call: the waiting agent loop runs the tool with the exact arguments that were shown, and the answer continues. `feedback` reaches the agent. With `remember: "chat"`, the same tool also runs without asking in that chat for `minutes` (default 60, at most 1440); see [`memory.grants`](../../ai-inference/memory/memory.grants.md).
- A CEP call (`system.exec`, a path outside the roots): the caller sends the same call again with `"_meta": {"approval": "<id>"}`, and it runs once.

Answers the approval (`status: "approved"`) and the chat grant, if one was made. Only the owner may call it.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"approvals.approve","params":{"approval_id":"ap_41c2…","remember":"chat","minutes":30}}'
```

Leave out the `Authorization` header when API authentication is off.
