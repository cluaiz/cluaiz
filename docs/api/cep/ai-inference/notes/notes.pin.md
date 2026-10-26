---
method: "notes.pin"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "Pins or unpins a note; pinned notes go into the system prompt of new chats"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "id"
    type: "string"
    req: true
    desc: "The note's `id` (from `notes.list`)"
    example: "n7"
  - name: "pinned"
    type: "boolean"
    req: true
    desc: "true: goes into the system prompt of new chats; false: does not"
    example: true
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.pin",
    "params": {
      "id": "n7",
      "pinned": true
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
  - code: -32004
    type: "note_not_found"
    when: "No note with this id"
  - code: -32010
    type: "notes_unavailable"
    when: "The notes file could not be written"
---

# notes.pin

Pins or unpins a note. A pinned confirmed note goes into the system prompt of every new chat in its scope. Answers the note.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"notes.pin","params":{"id":"n7","pinned":true}}'
```

Leave out the `Authorization` header when API authentication is off.
