---
method: "notes.create"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "A note written by the user, confirmed at once; it supersedes a live note on the same subject"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "The chat, for scope chat"
    example: null
  - name: "pinned"
    type: "boolean"
    req: false
    desc: "Pinned notes go into the system prompt of new chats"
    example: null
  - name: "scope"
    type: "string | null"
    req: false
    desc: "chat, workspace (default) or global"
    example: "workspace"
  - name: "text"
    type: "string"
    req: true
    desc: "1 to 2000 characters"
    example: "Prefers tabs over spaces"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.create",
    "params": {
      "scope": "workspace",
      "text": "Prefers tabs over spaces"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "note": {
        "id": "n7",
        "pinned": false,
        "scope": "workspace",
        "status": "confirmed",
        "text": "Prefers tabs over spaces"
      },
      "superseded": []
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`text` is empty or over 2000 characters, `scope` is unknown, or scope chat has no `chat_id`"
  - code: -32010
    type: "notes_unavailable"
    when: "The notes file could not be written"
---

# notes.create

A note written by the user. It is confirmed at once, and it supersedes a live note on the same subject (answered in `superseded`).

Notes are durable facts and preferences that outlive a chat ("prefers tabs over spaces"). Each has a `scope`: `chat` (one chat), `workspace` (the current workspace, the default) or `global`; and a `status`: `proposed` (the model suggested it), `confirmed`, `superseded` (a newer note on the same subject replaced it) or `invalidated`. Confirmed notes that apply go into the model's context; pinned ones go into the system prompt of new chats.

`text` is 1 to 2000 characters. `scope: "chat"` needs `chat_id`. `pinned: true` puts it into the system prompt of new chats.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"notes.create","params":{"text":"Deploys go to port 8443","scope":"workspace","pinned":true}}'
```

Leave out the `Authorization` header when API authentication is off.
