---
method: "providers.remove"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "Removes a provider, with its key if it was kept in the file"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: true
idempotent: false
params:
  - name: "name"
    type: "string"
    req: true
    desc: "Provider name"
    example: "myapi"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "providers.remove",
    "params": {
      "name": "myapi"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "removed": "myapi"
    }
  }
errors:
  - code: -32004
    type: "provider_not_found"
    when: "No provider has this name"
  - code: -32010
    type: "settings_not_saved"
    when: "providers.json could not be written"
---

# providers.remove

Removes a provider from `config/providers.json`. A key kept in the file goes with it; an environment variable named in `api_key_env` is not touched.

Requests for the removed provider's models then fail as for any unknown model.

## Related

- [`providers.list`](providers.list.md), [`providers.set`](providers.set.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
