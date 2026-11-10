# Model providers

A model provider is another service with an OpenAI-compatible API, on the internet or on the same computer. Once a provider is added, its chat models work in `cluaiz run`, `cluaiz agent` and `/v1/chat/completions`, and its embedding, text-to-speech and speech-to-text models in `/v1/embeddings`, `/v1/audio/speech` and `/v1/audio/transcriptions`, next to the models installed in Cluaiz.

A provider's model is named `<provider>/<model>`: `myapi/model-name` is the model `model-name` at the provider named `myapi`. The names in this page are placeholders: use your service's base URL, key and model ids.

## Add a provider

```bash
cluaiz provider add myapi --url https://api.example.com/v1 --key-env MYAPI_KEY
```

- `--url` is the provider's OpenAI-compatible base URL. The Engine adds `/chat/completions` and `/models` to it.
- `--key-env` names the environment variable that holds the key. The key stays out of the file. The Engine also reads the variable from `config/.env` in the Cluaiz folder (`~/.cluaiz/config/.env`), as a line `MYAPI_KEY=<key>`.
- `--key <key>` keeps the key in `providers.json` instead; `--key -` reads it from stdin, which keeps it out of the shell history.
- A provider that needs no key, such as Ollama on the same computer (`--url http://localhost:11434/v1`), needs neither.
- `--model <id>=<context>` lists a chat model with its context size, such as `--model model-name=32768`.
- `--embedding <id>`, `--tts <id>` and `--stt <id>` list an embedding, text-to-speech or speech-to-text model. Listing gives the model its kind: `/v1/models` shows it in that category, and the other routes answer with a clear error instead of sending it to the provider. A model that is not listed works with any route.

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
| `models` | Listed models: `context` (tokens) and `kind` (`chat`, the default, `embedding`, `tts` or `stt`). Other models of the provider work too |

A model that is not listed gets its context size from the provider's model list the first time `cluaiz run` or `cluaiz agent` uses it, and it is then recorded in the file. Without one, 32768 tokens is assumed.

Apps manage the same file through CEP: [`providers.list`](../api/cep/models/providers/providers.list.md), [`providers.set`](../api/cep/models/providers/providers.set.md), [`providers.remove`](../api/cep/models/providers/providers.remove.md), [`providers.models`](../api/cep/models/providers/providers.models.md) and [`providers.test`](../api/cep/models/providers/providers.test.md).

## Keys and API authentication

A key is sent only to its provider's own URL, in the `Authorization` header. It is never logged, and every list and error shows it masked or names only where it comes from (`env MYAPI_KEY`, `file sk-a…a1b2`).

API authentication stays your setting. When it is off and a provider has a key, anyone who can reach the server can use that key through it. `cluaiz provider add`, `cluaiz provider list`, `cluaiz serve` and the CEP replies then say so. On your own computer that is often fine; on a shared network or a server, turn authentication on with `cluaiz token create`, and send `Authorization: Bearer <token>` with each request.

## What works with a provider's model

A provider's chat model runs through the Engine's own chat, as an installed model does: the Engine prepares the conversation (memory, notes, the chat's ledger, Engine tools, context planning) and the provider generates the answer.

| | Installed model | Provider's model |
|---|---|---|
| Chat, streaming | Yes | Yes |
| Engine memory: `session_id`, `history: "engine"`, `storage`, the chat's events, notes, recall | Yes | Yes |
| Engine tools: tools bound by the request or the chat, suggested tools, `load_tool`, approvals | Yes | Yes, through the provider's function calling |
| Tools sent by an app as OpenAI `function`s | Yes | Yes: the calls come back to the app to run |
| Agent tools (`cluaiz agent`) | Yes | Yes |
| Context planning: the oldest turns left out to fit, agent auto-compact | Yes | Yes, with the context in providers.json; when the provider still says the request is too long, its oldest turns are left out and it is sent again |
| Attachments: images and sounds (API and terminal) | Yes | Yes, when the provider's model reads them; files arrive as their text |
| `temperature`, `top_p`, `presence_penalty`, `frequency_penalty`, `seed`, `stop`, `response_format`, `max_tokens` | Yes | Yes: from the request or the flags, else settings.json for the samplers |
| `chat.cancel` on a stream | Yes | Yes |
| Thinking shown | Yes | When the provider sends it (`reasoning_content`, `reasoning`, or `<think>` tags); `think_mode: "off"` / `--think off` hides it |
| `reasoning_effort` | Yes | `low`, `medium` and `high` only |
| Context size: `-c` in the terminal, `llama.n_ctx` in a request | Sets the loaded context | `-c` sets how much of the conversation is kept; `llama.n_ctx` is a load setting and does not apply. The provider's own limit still applies |
| Load settings: GPU layers, VRAM and RAM buffers, KV cache, threads, batch, mmap, flash attention, `keep_alive` | Yes | No: the provider runs the model. A terminal flag given is named in a note |
| `top_k`, `min_p`, `repeat_penalty`, `grammar` | Yes | No: not part of the OpenAI format |
| `chat.skip_reasoning` | Yes | No: it answers `reasoning_unavailable` |
| `models.load`, `models.inspect`, `models.remove`, `models.download`, `system.plan` | Yes | No: they answer `provider_model` |
| The chat's KV cache, idle-time cards written by the model | Yes | No: nothing is loaded here |

A model that calls a function it was not given (such as a suggested tool it did not load first) is refused by some providers. The Engine then answers that call itself, as it does for an installed model, so the turn goes on.

Errors from the provider have `"type": "provider_error"` (HTTP 502 for a whole answer) and say the cause in plain words: a refused key, an unknown model, a rate limit; a full context is `exceed_context_size_error`.

## Embeddings, speech and transcription

| Route | Sent to the provider | What comes back |
|---|---|---|
| `/v1/embeddings` | `{base_url}/embeddings` with `input`, `encoding_format`, `dimensions`, `user` | The provider's answer, with `model` set to the full name |
| `/v1/audio/speech` | `{base_url}/audio/speech` with `input`, `voice`, `speed`, WAV audio | The Engine's own shape: `output.audio_data` as `data:audio/wav;base64,…`; with `stream`, one `speech.audio.delta` event and the `done` event |
| `/v1/audio/transcriptions` | `{base_url}/audio/transcriptions` as a file upload, with `language`, `prompt`, `temperature` | The Engine's own shape: `output.text`, and `output.segments` with `timestamps: true`; with `stream`, the `done` event |

For transcription, the audio is read as for an installed model: `input_source` or `file` may be a data URI, a URL or an allowed local file, in any format the Engine decodes. It is sent to the provider as one-channel WAV.

Each provider has its own voices, languages and limits. The Engine passes `voice` on as it is; the provider's answer says when one is not accepted, and many providers need one. A WAV whose header has no length (as streamed audio often has) gets its real sizes, so players and the Engine read it whole. Fields a provider does not take are named in the `X-Cluaiz-Ignored` response header, and `X-Cluaiz-Provider` names the provider.

Engine memory, document ingest and workspace indexes keep using an installed embedding model: vectors from different models do not mix, and these features would otherwise send every text they index to the provider. `cluaiz model set-vector` therefore takes an installed model only.

`cluaiz provider test <name> <model>` sends a request of the model's listed kind: a short text for an embedding or text-to-speech model (`--voice <name>` when the provider needs a voice), a chat message otherwise. A speech-to-text model is tested by sending audio to `/v1/audio/transcriptions`.

## Not yet

- Providers with their own request format, not OpenAI-compatible. A service that also offers an OpenAI-compatible URL can be used through that URL.
- A settings page in the App.
