---
method: "providers.test"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "Sends one short chat request: the URL, the key and the model work"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "model"
    type: "string | null"
    req: false
    desc: "The model to try; default: the first listed one"
    example: "model-name"
  - name: "name"
    type: "string"
    req: true
    desc: "Provider name"
    example: "myapi"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "providers.test",
    "params": {
      "model": "model-name",
      "name": "myapi"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "answer": "ready",
      "model": "myapi/model-name",
      "ms": 420
    }
  }
errors:
  - code: -32004
    type: "provider_not_found"
    when: "No provider has this name"
  - code: -32602
    type: "invalid_params"
    when: "No model was sent and the provider lists none"
  - code: -32010
    type: "provider_unavailable"
    when: "The provider did not answer, or refused the request"
---

# providers.test

Sends one short chat request to the provider ("Reply with the single word: ready", at most 64 tokens) to check that the URL, the key and the model work. The result has the model's answer and the time the request took in `ms`.

`model` is the model id at the provider, such as `model-name`. Without it, the first listed model is used.

The request is a real request to the provider and counts toward its usage limits.

Errors name the cause in plain words: a refused key (401 or 403), an unknown model (404) or a rate limit (429), followed by the provider's own message. The key is never part of an error.

## Related

- [`providers.models`](providers.models.md), [`providers.set`](providers.set.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
- [api/src/providers.rs](../../../../../api/src/providers.rs)
