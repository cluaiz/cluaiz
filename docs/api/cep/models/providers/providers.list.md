---
method: "providers.list"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "The model providers and their listed models; keys are never returned"
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
    "method": "providers.list",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "providers": [
        {
          "base_url": "https://api.example.com/v1",
          "key": "env MYAPI_KEY",
          "models": [
            {
              "context": 32768,
              "id": "model-name",
              "model": "myapi/model-name"
            }
          ],
          "name": "myapi",
          "type": "openai"
        }
      ]
    }
  }
errors: []
---

# providers.list

Lists the model providers in `config/providers.json` and the models listed for each. A provider's model is used by its full name, `<provider>/<model>`, in `/v1/chat/completions`, `cluaiz run` and `cluaiz agent -m`.

## Result

| Field | Meaning |
|---|---|
| `providers[].name` | The provider's name, the first part of its model names |
| `providers[].type` | Request format. Only `openai` (OpenAI Chat Completions) for now |
| `providers[].base_url` | The provider's OpenAI-compatible base URL |
| `providers[].key` | Where the key comes from (see below) |
| `providers[].models[]` | Listed models: `id` at the provider, `model` as the full name, `context` in tokens |
| `warning` | Present only when API authentication is off and a provider has a key |

Replies never contain a key. `key` says only where it comes from: `env MYAPI_KEY` (an environment variable, also read from `config/.env` in the Cluaiz folder), `file sk-a…a1b2` (kept in providers.json, shown masked) or `none`.

When API authentication is off (`api_auth.required` is false) and a provider has a key, the result also has a `warning`: anyone who can reach the server can then use that key. Authentication stays the owner's setting; this method does not change it. Turn it on with `cluaiz token create` or `tokens.create` plus `api_auth.required`.

A provider's other models can be used too; listing a model only records its context size.

## Related

- [`providers.set`](providers.set.md), [`providers.remove`](providers.remove.md), [`providers.models`](providers.models.md), [`providers.test`](providers.test.md)
- Guide: [Model providers](../../../../engine/providers.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
