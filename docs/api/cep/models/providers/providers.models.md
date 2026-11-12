---
method: "providers.models"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "The models a provider offers (its GET /models), with the context size it reports"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
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
    "method": "providers.models",
    "params": {
      "name": "myapi"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "models": [
        {
          "context": 32768,
          "id": "model-name",
          "model": "myapi/model-name"
        }
      ]
    }
  }
errors:
  - code: -32004
    type: "provider_not_found"
    when: "No provider has this name"
  - code: -32010
    type: "provider_unavailable"
    when: "The provider did not answer, or refused the key"
---

# providers.models

Asks the provider for its models (`GET {base_url}/models`, with its key) and returns each one with its full name. `context` is the context size the provider reports, read from the field it uses (`context_length`, `context_window` and similar); it is `null` when the provider reports none.

This method does not change providers.json. To record a model's context size, send it in `providers.set`. `cluaiz run` and `cluaiz agent` also record it the first time they use a model.

When the provider does not answer or refuses the key, the error is `provider_unavailable` with the provider's own message; the key is never part of it.

## Related

- [`providers.set`](providers.set.md), [`providers.test`](providers.test.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
- [api/src/providers/models.rs](../../../../../api/src/providers/models.rs)
