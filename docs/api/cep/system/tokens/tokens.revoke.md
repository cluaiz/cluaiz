---
method: "tokens.revoke"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tokens"
summary: "Removes an API key; with none left, API authentication turns off (as before)"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: true
idempotent: false
params:
  - name: "index"
    type: "integer | null"
    req: false
    desc: "Or its position in `tokens.list`, from 1"
    example: 1
  - name: "token"
    type: "string | null"
    req: false
    desc: "The key itself, or its masked form (at least 4 visible trailing characters)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tokens.revoke",
    "params": {
      "index": 1
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "api_auth_required": false,
      "revoked_token": "sk-cluaiz-1f0c…",
      "tokens": []
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "Neither `token` nor `index` was sent"
  - code: -32004
    type: "token_not_found"
    when: "No key matches `token`, or `index` is out of range"
  - code: -32010
    type: "settings_not_saved"
    when: "permission.json could not be written"
---

# tokens.revoke

Removes one API key, chosen by the key itself (or its masked form with at least 4 visible trailing characters) or by its position in [`tokens.list`](tokens.list.md), from 1.

When the last key is removed, `api_auth.required` is turned off so no client is locked out; the Engine log says so.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tokens.revoke","params":{"index":1}}'
```

Leave out the `Authorization` header when API authentication is off.
