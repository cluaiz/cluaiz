---
method: "tokens.list"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tokens"
summary: "The API keys, and whether API authentication is required"
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
    "method": "tokens.list",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "api_auth_required": false,
      "tokens": [
        "sk-cluaiz-1f0c…"
      ]
    }
  }
errors: []
---

# tokens.list

The API keys in `permission.json` and whether API authentication is on. Keys and the on/off switch are the owner's settings.

## Result

`tokens` (the keys, `sk-cluaiz-…`) and `api_auth_required`. The session token (`session.token` in the Engine home) is not an API key and is not listed.

Admin scope, owner only in practice, never an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tokens.list","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
