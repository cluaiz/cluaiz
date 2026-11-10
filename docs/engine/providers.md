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
| Agent auto-compact when the context is nearly full | Yes | Yes, from the token counts the provider reports |
| Oldest messages left out when the context is full | Yes | Yes: in `cluaiz run`, in the agent and in `/v1/chat/completions` |
| `temperature`, `top_p`, `presence_penalty`, `frequency_penalty`, `seed` | Yes | Yes: from the request or the flags, else from settings.json |
| `chat.cancel` on a stream | Yes | Yes |
| Thinking shown | Yes | When the provider sends it (`reasoning_content`, `reasoning`, or `<think>` tags); `--think off` hides it |
| `--reasoning-effort` / `reasoning_effort` | Yes | `low`, `medium` and `high` only |
| Context size (`-c`) | Sets the loaded context | Sets how much of the conversation is kept; the provider's own limit still applies |
| Load settings: GPU layers, VRAM and RAM buffers, KV cache, threads, batch, mmap, flash attention | Yes | No: the provider runs the model. A flag given is named in a note |
| `top_k`, `min_p`, `repeat_penalty` | Yes | No: not part of the OpenAI format. A flag given is named in a note |
| `chat.skip_reasoning` | Yes | No: it answers `reasoning_unavailable` |
| `models.load`, `models.inspect`, `models.remove`, `models.download`, `system.plan` | Yes | No: they answer `provider_model` |
| Engine memory, `session_id`, server-side tools, context planning, KV cache | Yes | No: the request goes to the provider |
| Files and images in the terminal | Yes | Not yet |

## What `/v1/chat/completions` does with a provider's model

1. It sends the OpenAI fields of the request (`messages`, `tools`, `temperature`, `max_tokens` and the like). The Engine's own fields (`session_id`, `think_mode`, `top_k`, …) are left out and named in the `X-Cluaiz-Ignored` response header.
2. Sampler fields the request leaves out come from settings.json, as for an installed model.
3. When the provider says the request does not fit its context, the oldest turns are left out (system messages and the last question stay) and the request is sent again. The `X-Cluaiz-Dropped-Messages` header says how many messages went.
4. The answer comes back with `model` set to the full name (`myapi/model-name`). A stream's chunks carry this request's id, so `chat.cancel` with that `stream_id` stops it.
5. `X-Cluaiz-Provider` names the provider. Errors are `{"error": {"type": "provider_error", "provider": "...", "message": "..."}}` with the provider's status code, and say the cause in plain words: a refused key, an unknown model, a rate limit or a full context.

## Not yet

- Providers with their own request format, not OpenAI-compatible. A service that also offers an OpenAI-compatible URL can be used through that URL.
- Files and images sent to a provider's model from the terminal.
- Engine memory for a provider's chats, and its server-side tools.
- A settings page in the App.
