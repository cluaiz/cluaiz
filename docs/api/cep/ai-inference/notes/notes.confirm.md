---
method: "notes.confirm"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "A proposed note becomes confirmed (and supersedes a live note on the same subject)"
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
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.confirm",
    "params": {
      "id": "n7"
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

# notes.confirm

Confirms a proposed note (one the model suggested with `remember`, under the `config` context setting `notes: ask`). It then takes part in the model's context, and supersedes a live note on the same subject. Answers the note and the notes it superseded.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"notes.confirm","params":{"id":"n7"}}'
```

Leave out the `Authorization` header when API authentication is off.
