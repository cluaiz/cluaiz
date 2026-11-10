# Model providers

A model provider is another service with an OpenAI-compatible API, on the internet or on the same computer. Once a provider is added, its models work in `cluaiz run`, `cluaiz agent` and `/v1/chat/completions`, next to the models installed in Cluaiz.

A provider's model is named `<provider>/<model>`: `myapi/model-name` is the model `model-name` at the provider named `myapi`. The names in this page are placeholders: use your service's base URL, key and model ids.

## Add a provider

```bash
cluaiz provider add myapi --url https://api.example.com/v1 --key-env MYAPI_KEY
```

- `--url` is the provider's OpenAI-compatible base URL. The Engine adds `/chat/completions` and `/models` to it.
- `--key-env` names the environment variable that holds the key. The key stays out of the file. The Engine also reads the variable from `config/.env` in the Cluaiz folder (`~/.cluaiz/config/.env`), as a line `MYAPI_KEY=<key>`.
- `--key <key>` keeps the key in `providers.json` instead; `--key -` reads it from stdin, which keeps it out of the shell history.
- A provider that needs no key, such as Ollama on the same computer (`--url http://localhost:11434/v1`), needs neither.
- `--model <id>=<context>` lists a model with its context size, such as `--model model-name=32768`.

The base URL is in your service's documentation; it usually ends in `/v1`.

## Use its models

```bash
cluaiz provider models myapi           # the models it offers, with their context size
cluaiz provider test myapi model-name
cluaiz run myapi/model-name "Explain this error"
cluaiz agent -m myapi/model-name
cluaiz model set-chat myapi/model-name   # the default for `cluaiz agent` and model "auto"
```

In the agent, `/model myapi/<model>` switches to a provider's model and `/model <installed id>` switches back; the conversation is kept.

From an app, send the full name as `model`:

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "myapi/model-name", "messages": [{"role": "user", "content": "Hello"}], "stream": true}'
```

`/v1/models` lists the models in providers.json too, with `provider` and `context_window`.

## The file

`providers.json` in the Cluaiz config folder (`cluaiz provider list` prints its path):

```json
{
  "providers": {
    "myapi": {
      "type": "openai",
      "base_url": "https://api.example.com/v1",
      "api_key_env": "MYAPI_KEY",
      "models": { "model-name": { "context": 32768 } }
    },
    "ollama": {
      "type": "openai",
      "base_url": "http://localhost:11434/v1"
    }
  }
}
```

| Field | Meaning |
|---|---|
| `type` | Request format. Only `openai` (OpenAI Chat Completions) for now |
| `base_url` | OpenAI-compatible base URL |
| `api_key_env` | Name of the environment variable that holds the key |
| `api_key` | The key itself, when it is kept in the file. On Linux and macOS the file is then readable by its owner only |
| `models` | Listed models and their context size in tokens. Other models of the provider work too |

A model that is not listed gets its context size from the provider's model list the first time `cluaiz run` or `cluaiz agent` uses it, and it is then recorded in the file. Without one, 32768 tokens is assumed.

Apps manage the same file through CEP: [`providers.list`](../api/cep/models/providers/providers.list.md), [`providers.set`](../api/cep/models/providers/providers.set.md), [`providers.remove`](../api/cep/models/providers/providers.remove.md), [`providers.models`](../api/cep/models/providers/providers.models.md) and [`providers.test`](../api/cep/models/providers/providers.test.md).

## Keys and API authentication

A key is sent only to its provider's own URL, in the `Authorization` header. It is never logged, and every list and error shows it masked or names only where it comes from (`env MYAPI_KEY`, `file sk-a…a1b2`).

API authentication stays your setting. When it is off and a provider has a key, anyone who can reach the server can use that key through it. `cluaiz provider add`, `cluaiz provider list`, `cluaiz serve` and the CEP replies then say so. On your own computer that is often fine; on a shared network or a server, turn authentication on with `cluaiz token create`, and send `Authorization: Bearer <token>` with each request.

## What works with a provider's model

| | Installed model | Provider's model |
|---|---|---|
| Chat, streaming | Yes | Yes |
| Agent tools (`cluaiz agent`) | Yes | Yes, through the provider's function calling |
| Tools sent by an app in `tools` | Yes | Yes: the calls come back to the app to run |
| Thinking shown | Yes | When the provider sends it (`reasoning_content`, `reasoning`, or `<think>` tags) |
| Files and images in the terminal | Yes | Not yet |
| Engine memory, `session_id`, server-side tools, context planning, KV cache | Yes | No: the request goes to the provider as it is |

For `/v1/chat/completions`, the Engine sends the OpenAI fields of the request (`messages`, `tools`, `temperature`, `max_tokens` and the like) and leaves out its own (`session_id`, `think_mode`, `top_k`, …). The provider's answer comes back as the provider sent it, so `model` in it is the provider's model id.

## Not yet

- Providers with their own request format, such as Anthropic's Messages API, Google Vertex AI and AWS Bedrock. A service that also offers an OpenAI-compatible URL can be used through that URL.
- Files and images sent to a provider's model from the terminal.
- A settings page in the App.
