---
method: "providers.set"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "Adds a provider, or changes its URL, key source and listed models"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "api_key"
    type: "string | null"
    req: false
    desc: "The key itself, kept in providers.json. Leave both out to keep the current key"
    example: null
  - name: "api_key_env"
    type: "string | null"
    req: false
    desc: "Name of the environment variable that holds the key, such as `MYAPI_KEY`"
    example: "MYAPI_KEY"
  - name: "base_url"
    type: "string"
    req: true
    desc: "OpenAI-compatible base URL, such as `https://api.example.com/v1`"
    example: "https://api.example.com/v1"
  - name: "models"
    type: "object | null"
    req: false
    desc: "Models to list, with their context size in tokens: `{\"model-name\": {\"context\": 32768}}`"
    example: {"model-name":{"context":32768}}
  - name: "name"
    type: "string"
    req: true
    desc: "Provider name, the first part of its model names (`myapi` → `myapi/<model>`): letters, digits, - and _"
    example: "myapi"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "providers.set",
    "params": {
      "api_key_env": "MYAPI_KEY",
      "base_url": "https://api.example.com/v1",
      "models": {
        "model-name": {
          "context": 32768
        }
      },
      "name": "myapi"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "provider": {
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
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "The name or the URL is not valid, or both api_key and api_key_env were sent"
  - code: -32010
    type: "settings_not_saved"
    when: "providers.json could not be written"
---

# providers.set

Adds a provider to `config/providers.json`, or changes one with the same `name`.

## Parameters

| Parameter | Required | Meaning |
|---|---|---|
| `name` | Yes | Letters, digits, `-` and `_`. It becomes the first part of the model names: `myapi` gives `myapi/<model>` |
| `base_url` | Yes | OpenAI-compatible base URL, such as `https://api.example.com/v1`. The Engine adds `/chat/completions` and `/models` |
| `api_key_env` | No | Name of the environment variable that holds the key. The key itself stays out of the file |
| `api_key` | No | The key, kept in providers.json. Send this or `api_key_env`, not both |
| `models` | No | Listed models with their context size: `{"model-name": {"context": 32768}}`. When sent, it replaces the list |

When neither `api_key_env` nor `api_key` is sent, the provider keeps its current key source. A provider that needs no key, such as Ollama on the same computer (`http://localhost:11434/v1`), needs neither.

Replies never contain a key. `key` says only where it comes from: `env MYAPI_KEY` (an environment variable, also read from `config/.env` in the Cluaiz folder), `file sk-a…a1b2` (kept in providers.json, shown masked) or `none`.

When API authentication is off (`api_auth.required` is false) and a provider has a key, the result also has a `warning`: anyone who can reach the server can then use that key. Authentication stays the owner's setting; this method does not change it. Turn it on with `cluaiz token create` or `tokens.create` plus `api_auth.required`.

## Related

- [`providers.list`](providers.list.md), [`providers.test`](providers.test.md)
- Guide: [Model providers](../../../../engine/providers.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
