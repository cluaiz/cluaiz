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
    desc: "Name of the environment variable that holds the key, such as `GROQ_API_KEY`"
    example: "GROQ_API_KEY"
  - name: "base_url"
    type: "string"
    req: true
    desc: "OpenAI-compatible base URL, such as `https://api.groq.com/openai/v1`"
    example: "https://api.groq.com/openai/v1"
  - name: "models"
    type: "object | null"
    req: false
    desc: "Models to list, with their context size in tokens: `{\"qwen/qwen3-32b\": {\"context\": 131072}}`"
    example: {"qwen/qwen3-32b":{"context":131072}}
  - name: "name"
    type: "string"
    req: true
    desc: "Provider name, the first part of its model names (`groq` → `groq/<model>`): letters, digits, - and _"
    example: "groq"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "providers.set",
    "params": {
      "api_key_env": "GROQ_API_KEY",
      "base_url": "https://api.groq.com/openai/v1",
      "models": {
        "qwen/qwen3-32b": {
          "context": 131072
        }
      },
      "name": "groq"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "provider": {
        "base_url": "https://api.groq.com/openai/v1",
        "key": "env GROQ_API_KEY",
        "models": [
          {
            "context": 131072,
            "id": "qwen/qwen3-32b",
            "model": "groq/qwen/qwen3-32b"
          }
        ],
        "name": "groq",
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
| `name` | Yes | Letters, digits, `-` and `_`. It becomes the first part of the model names: `groq` gives `groq/<model>` |
| `base_url` | Yes | OpenAI-compatible base URL, such as `https://api.groq.com/openai/v1`. The Engine adds `/chat/completions` and `/models` |
| `api_key_env` | No | Name of the environment variable that holds the key. The key itself stays out of the file |
| `api_key` | No | The key, kept in providers.json. Send this or `api_key_env`, not both |
| `models` | No | Listed models with their context size: `{"qwen/qwen3-32b": {"context": 131072}}`. When sent, it replaces the list |

When neither `api_key_env` nor `api_key` is sent, the provider keeps its current key source. A provider that needs no key, such as a local Ollama at `http://localhost:11434/v1`, needs neither.

Replies never contain a key. `key` says only where it comes from: `env GROQ_API_KEY` (an environment variable, also read from `config/.env` in the Cluaiz folder), `file gsk_…a1b2` (kept in providers.json, shown masked) or `none`.

When API authentication is off (`api_auth.required` is false) and a provider has a key, the result also has a `warning`: anyone who can reach the server can then use that key. Authentication stays the owner's setting; this method does not change it. Turn it on with `cluaiz token create` or `tokens.create` plus `api_auth.required`.

## Examples of base URLs

| Service | `base_url` |
|---|---|
| Groq | `https://api.groq.com/openai/v1` |
| OpenRouter | `https://openrouter.ai/api/v1` |
| OpenAI | `https://api.openai.com/v1` |
| Ollama on this computer | `http://localhost:11434/v1` |
| LM Studio on this computer | `http://localhost:1234/v1` |

These are the services' published OpenAI-compatible addresses; check the service's own documentation if one changes.

## Related

- [`providers.list`](providers.list.md), [`providers.test`](providers.test.md)
- Guide: [Model providers](../../../../engine/providers.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
