---
method: "tokens.create"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tokens"
summary: "Makes a new API key (at most 5)"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tokens.create",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "api_auth_required": false,
      "token": "sk-cluaiz-1f0c…",
      "tokens": [
        "sk-cluaiz-1f0c…"
      ]
    }
  }
errors:
  - code: -32005
    type: "token_limit"
    when: "There are 5 keys already: revoke one first"
  - code: -32010
    type: "settings_not_saved"
    when: "permission.json could not be written"
---

# tokens.create

Makes a new API key (`sk-cluaiz-` and 32 random hex characters), saves it in `permission.json`, and answers it with the full list. At most 5 keys exist at once.

Creating a key does not turn authentication on; that stays the owner's `api_auth.required` setting.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tokens.create","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
