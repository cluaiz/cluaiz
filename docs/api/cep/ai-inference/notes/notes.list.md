---
method: "notes.list"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "Notes, newest last"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "Only notes that apply to this chat (with the current workspace)"
    example: null
  - name: "scope"
    type: "string | null"
    req: false
    desc: "chat, workspace or global"
    example: null
  - name: "status"
    type: "string | null"
    req: false
    desc: "proposed, confirmed, superseded or invalidated"
    example: "confirmed"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.list",
    "params": {
      "status": "confirmed"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "notes": [
        {
          "id": "n7",
          "pinned": false,
          "scope": "workspace",
          "status": "confirmed",
          "text": "Prefers tabs over spaces"
        }
      ],
      "workspace": "D:/work"
    }
  }
errors: []
---

# notes.list

The notes, oldest first, optionally filtered.

Notes are durable facts and preferences that outlive a chat ("prefers tabs over spaces"). Each has a `scope`: `chat` (one chat), `workspace` (the current workspace, the default) or `global`; and a `status`: `proposed` (the model suggested it), `confirmed`, `superseded` (a newer note on the same subject replaced it) or `invalidated`. Confirmed notes that apply go into the model's context; pinned ones go into the system prompt of new chats.

## Filters

`scope`, `status`, and `chat_id` (only the notes that apply to that chat in the current workspace: its chat notes, the workspace's, and the global ones). Answers `workspace` (the current workspace) and `notes[]`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"notes.list","params":{"status":"confirmed"}}'
```

Leave out the `Authorization` header when API authentication is off.
