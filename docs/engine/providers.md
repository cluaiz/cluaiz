# Model providers

A model provider is another service with an OpenAI-compatible API or the Anthropic Messages API, on the internet or on the same computer. Once a provider is added, its chat models work in `cluaiz run`, `cluaiz agent` and `/v1/chat/completions`, and its embedding, text-to-speech and speech-to-text models in `/v1/embeddings`, `/v1/audio/speech` and `/v1/audio/transcriptions`, next to the models installed in Cluaiz.

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
- `--type anthropic` for a service that speaks the Anthropic Messages API at a URL the Engine does not know (Anthropic's own URLs are recognized without it).
- `--auth <how>` when the key goes in another header than the URL suggests: `bearer`, `api-key`, `x-api-key`, `google`, `aws`, `azure` or `none`.

The base URL is in your service's documentation; it usually ends in `/v1`.

## Cloud platforms and other model makers

The same commands reach models of many companies through the platforms that host them:

| Platform | Base URL | Key | Models |
|---|---|---|---|
| Anthropic (Claude) | `https://api.anthropic.com/v1` | `--key-env ANTHROPIC_API_KEY` | `claude-opus-5-5`, `claude-sonnet-5-5`, `claude-haiku-5-5`, … |
| Google AI Studio (Gemini API) | `https://generativelanguage.googleapis.com/v1beta/openai` | `--key-env GEMINI_API_KEY` | Google's own: Gemini, Gemma |
| Google Vertex AI, OpenAI URL | `https://aiplatform.googleapis.com/v1/projects/<project>/locations/global/endpoints/openapi` | None: a Google Cloud sign-in (below) | `google/gemini-3.5-flash`, and the open models enabled in the project's Model Garden |
| Google Vertex AI, Claude | `https://<region>-aiplatform.googleapis.com/v1/projects/<project>/locations/<region>` (or `aiplatform.googleapis.com/…/locations/global`) | None: a Google Cloud sign-in | `claude-sonnet-5-5`, … (enabled and with quota in the project) |
| Amazon Bedrock, OpenAI URL | `https://bedrock-runtime.<region>.amazonaws.com/openai/v1` | A Bedrock API key, `--key-env AWS_BEARER_TOKEN_BEDROCK`, or no key: the AWS credentials (below) | Models with Chat Completions on Bedrock, such as `openai.gpt-oss-120b-1:0` |
| Claude in Amazon Bedrock | `https://bedrock-mantle.<region>.api.aws/anthropic/v1` | A Bedrock bearer token, `--key-env AWS_BEARER_TOKEN_BEDROCK`, or no key: the AWS credentials | `anthropic.claude-opus-5-5`, … |
| Azure OpenAI, Microsoft Foundry | `https://<resource>.openai.azure.com/openai/v1` (or `.services.ai.azure.com/openai/v1`) | `--key-env AZURE_OPENAI_API_KEY` (sent as `api-key`), or no key: a Microsoft Entra sign-in (below) | Your deployment names |
| OpenRouter, Together, Groq, DeepSeek, Mistral, xAI, … | Their OpenAI-compatible URL | Their key | Theirs |

A key belongs to its platform: a Google AI Studio key reaches Google's own models, and models of other companies on Google Cloud need a Vertex AI project with those models enabled.

**Google Cloud sign-in.** Vertex AI takes no key in providers.json. The Engine signs in as Google's own tools do: the service account file named in `GOOGLE_APPLICATION_CREDENTIALS`, else the login saved by `gcloud auth application-default login`, else `gcloud auth print-access-token`. The access token lasts an hour and is renewed on its own. `cluaiz provider list` shows the key as `Google Cloud sign-in`.

**AWS credentials.** A Bedrock URL without a key is signed with AWS Signature Version 4, as the AWS tools do: `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` (with `AWS_SESSION_TOKEN`), else the profile `AWS_PROFILE` (or `default`) in `~/.aws/credentials`, else `aws configure export-credentials` (single sign-on and roles; renewed before they end). The region comes from the URL; `bedrock-runtime` URLs sign for the service `bedrock`, `bedrock-mantle` URLs for `bedrock-mantle`. The account needs Bedrock access (`bedrock:InvokeModel`, or `bedrock-mantle:CreateInference`); without it the error says the sign-in was accepted and the model is not allowed. `cluaiz provider list` shows `AWS credentials`.

**Microsoft Entra sign-in.** An Azure URL without a key gets a Microsoft Entra ID token: a service principal in `AZURE_TENANT_ID`, `AZURE_CLIENT_ID` and `AZURE_CLIENT_SECRET`, else `az account get-access-token`. The token is for `https://cognitiveservices.azure.com/.default` (`AZURE_TOKEN_SCOPE` sets another), sent as `Authorization: Bearer`, and renewed before it ends. `cluaiz provider list` shows `Microsoft Entra sign-in`.

Not every platform has every route: Bedrock's `bedrock-runtime` URL and Claude on Vertex AI have no model list (`cluaiz provider models` answers with the platform's error; list the models with `--model`).

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
| `type` | Request format: `openai` (OpenAI Chat Completions, the default) or `anthropic` (Anthropic Messages). The URLs of the Claude API, Claude in Amazon Bedrock and Claude on Vertex AI are recognized as `anthropic` |
| `base_url` | The provider's base URL |
| `api_key_env` | Name of the environment variable that holds the key |
| `api_key` | The key itself, when it is kept in the file. On Linux and macOS the file is then readable by its owner only |
| `models` | Listed models: `context` (tokens) and `kind` (`chat`, the default, `embedding`, `tts` or `stt`). Other models of the provider work too |
| `compat` | Optional: how the provider differs from the OpenAI format, when the Engine does not tell it from the URL. See [Provider differences](#provider-differences) |
| `auth` | Optional: how the key is sent: `bearer` (`Authorization: Bearer`), `api-key` (Azure), `x-api-key` (Anthropic), `google` (a Google Cloud sign-in), `aws` (AWS credentials, each request signed), `azure` (a Microsoft Entra sign-in), `none`. Unset, the URL decides: Vertex AI `google`; Bedrock URLs `aws` without a key, else `bearer` (`x-api-key` for Claude); Azure URLs `azure` without a key, else `api-key`; Anthropic-format URLs `x-api-key`; others `bearer` |
| `headers` | Optional: headers sent with every request, such as `{"anthropic-beta": "…"}` or OpenRouter's `HTTP-Referer` |

A model that is not listed gets its context size from the provider's model list the first time `cluaiz run` or `cluaiz agent` uses it, and it is then recorded in the file. Without one, 32768 tokens is assumed.

Apps manage the same file through CEP: [`providers.list`](../api/cep/models/providers/providers.list.md), [`providers.set`](../api/cep/models/providers/providers.set.md), [`providers.remove`](../api/cep/models/providers/providers.remove.md), [`providers.models`](../api/cep/models/providers/providers.models.md) and [`providers.test`](../api/cep/models/providers/providers.test.md).

## Provider differences

Providers that call themselves OpenAI-compatible still differ: how thinking is turned on and off, which field holds the answer limit, the form of a tool call id, what must come back with a call. The Engine tells the known providers from their base URL and writes each request and reads each answer their way. Nothing needs to be set for them:

| Base URL contains | Thinking on/off and level | Other differences |
|---|---|---|
| `generativelanguage.googleapis.com` (Gemini) | Shown with `thinking_config` (`include_thoughts`, `thinking_level`); off with `reasoning_effort: "none"`. The thinking arrives between `<thought>` tags and is shown apart | Each call's `thought_signature` goes back with it |
| `deepseek.com` | `thinking.type`, and `reasoning_effort` | `reasoning_content` on messages with calls |
| `api.z.ai`, `bigmodel.cn` | `thinking.type` | No `reasoning_effort` |
| `together.ai`, `together.xyz` | `reasoning.enabled` | No `reasoning_effort` |
| `openrouter.ai` | `reasoning.effort` (`"none"` for off) | |
| `dashscope…aliyuncs.com` (Qwen) | `enable_thinking` | |
| `api.mistral.ai` | — | Tool call ids of nine letters and digits |
| `api.openai.com`, `openai.azure.com` | `reasoning_effort` | `max_completion_tokens` |
| `integrate.api.nvidia.com` (NVIDIA) | `chat_template_kwargs.enable_thinking` on or off, with `reasoning_effort` for the level (some models think only when it is sent) | |
| `api.x.ai`, `moonshot` | — | No `reasoning_effort` |
| `aiplatform.googleapis.com/…/endpoints/openapi` (Gemini on Vertex AI) | As Gemini; off sends nothing (this URL refuses `reasoning_effort`) | The thinking arrives in pieces marked `thought`, without tags |
| Anthropic Messages (`api.anthropic.com`, `bedrock-mantle…/anthropic`, Vertex AI Claude) | `thinking: {type: "adaptive", display: "summarized"}` with `output_config.effort`; off leaves `thinking` out and asks for `low` effort (newer Claude models cannot turn thinking off) | `max_tokens` is required (32000 when the request sets none, 8192 for a model that takes less); `temperature` and `top_p` are not sent (API version 2026-10-21 refuses them); each answer's thinking block goes back unchanged, with its signature, before that answer's calls |
| Anything else (Groq, Ollama, a server on this computer) | `reasoning_effort` | The plain OpenAI format |

Thinking off only asks the model not to think; it hides nothing. A model that thinks anyway has its thinking shown apart from the answer, as with thinking on. A model that cannot stop thinking and refuses the request (OpenRouter: "Reasoning is mandatory for this endpoint and cannot be disabled") gets the same request once more without the off field, and its thinking is shown. To hide thinking, use `--hide-thinking` (terminal) or ignore `reasoning_content` (API).

For a provider the Engine does not know, or to change what it detects, set `compat` in providers.json. Every field is optional:

```json
"localqwen": {
  "base_url": "http://localhost:8080/v1",
  "compat": { "thinking": "qwen-template" }
}
```

| `compat` field | Values | Meaning |
|---|---|---|
| `thinking` | `openai`, `gemini`, `qwen`, `qwen-template`, `nvidia`, `zai`, `deepseek`, `openrouter`, `together`, `none` | How thinking is turned on and off. `qwen-template` sends `chat_template_kwargs.enable_thinking` (Qwen on vLLM, SGLang or a llama.cpp server); `nvidia` sends it with `reasoning_effort`; `none` sends nothing |
| `reasoning_effort` | `true`, `false` | Whether `reasoning_effort` is sent |
| `max_tokens_field` | `max_tokens`, `max_completion_tokens` | The answer limit's field |
| `usage_in_stream` | `true`, `false` | Whether `stream_options.include_usage` is sent |
| `reasoning_content_on_calls` | `true`, `false` | Whether an assistant message with calls carries `reasoning_content` |
| `tool_call_ids` | `any`, `alnum9` | The form call ids must have |
| `content_not_null` | `true`, `false` | Whether an assistant message with calls has `""` instead of a null `content` |

A field the Engine adds on its own (`reasoning_effort`, `thinking`, `stream_options`, `seed`, `temperature`, `max_completion_tokens` and the like) that a provider refuses is left out and the request is sent once more without it; the server log names it, so it can be set in `compat`. The conversation, the tools and the answer schema are never left out.

### Thinking

The chat's thinking settings work for a provider's model as for an installed one:

| Setting | What the provider gets | What is shown |
|---|---|---|
| `think_mode: "auto"` (the default) | The level, when one is set; Gemini is asked to send its thinking | The thinking the model sends, apart from the answer |
| `think_mode: "on"` | Thinking turned on in the provider's own field, with the level | The thinking, apart from the answer |
| `think_mode: "off"` | Thinking turned off where the provider can (Gemini, DeepSeek, Qwen, z.ai, OpenRouter, Together); OpenAI-format models decide themselves | Nothing of the thinking |
| `reasoning_effort` | `low`, `medium`, `high`; `max` is sent as `high`, a budget in tokens as the closest level (up to 2048 low, up to 8192 medium) | |

Thinking arrives from providers in `reasoning_content`, `reasoning`, `reasoning_text`, or in the text between `<think>` or `<thought>` tags. All of them reach the app as `reasoning_content` and the terminal as thinking, never inside the answer. A model that decides not to think, or not to show it, sends none.

### When a request is sent again

| What happened | What the Engine does |
|---|---|
| Rate limit (429 with a per-minute or per-day limit) | Waits what the provider says (`Retry-After`, or "try again in 8.2s"), at most 20 seconds, up to two more times |
| Busy or failed on its side (5xx, 529, "overloaded") | Tries again after 1 second, then 3 |
| The connection did not open or timed out | Tries once more |
| The answer came with nothing at all (no text, thinking or call) | Asks once more |
| The request does not fit the context | The server's chat leaves out the oldest turns; the agent makes room itself |
| A spending cap, an empty balance, a refused key, an unknown model | No retry: the error says so in plain words |

Every wait ends when the person cancels.

### The code

| File in `api/src/providers/` | Job |
|---|---|
| `http.rs` | The client; the key in the header each provider takes |
| `google.rs` | The Google Cloud sign-in for Vertex AI |
| `aws.rs` | AWS credentials and Signature Version 4 for Amazon Bedrock |
| `azure.rs` | The Microsoft Entra sign-in for Azure |
| `anthropic.rs` | The Anthropic Messages format: the request, the conversation, the streamed answer |
| `compat.rs` | The differences above: detected from the URL, overridden by `compat` |
| `request.rs` | The chat request: samplers, answer limit, thinking, tools, answer schema |
| `history.rs` | The Engine's conversation as the provider's messages |
| `stream.rs` | The streamed answer as thinking, text and `<tool_call>` blocks |
| `errors.rs`, `retry.rs` | Failures in plain words, their kind, and when to try again |
| `chat.rs` | One chat request from start to end, for the server's chat and the terminal |
| `models.rs`, `media/` | Model lists, `provider test`, embeddings, speech and transcription |
| `tests/` | One recorded or documented stream per provider (`fixtures/`), read as the chat reads it |

## Keys and API authentication

A key is sent only to its provider's own URL, in the header that provider takes (`Authorization`, `api-key` or `x-api-key`). It is never logged, and every list and error shows it masked or names only where it comes from (`env MYAPI_KEY`, `file sk-a…a1b2`).

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
| Thinking on, off and shown | Yes | Yes, in each provider's own fields; see [Thinking](#thinking) |
| `reasoning_effort` | Yes | As a level; see [Thinking](#thinking) |
| Context size: `-c` in the terminal, `llama.n_ctx` in a request | Sets the loaded context | `-c` sets how much of the conversation is kept; `llama.n_ctx` is a load setting and does not apply. The provider's own limit still applies |
| Load settings: GPU layers, VRAM and RAM buffers, KV cache, threads, batch, mmap, flash attention, `keep_alive` | Yes | No: the provider runs the model. A terminal flag given is named in a note |
| `top_k`, `min_p`, `repeat_penalty`, `grammar` | Yes | No: not part of the OpenAI format |
| `chat.skip_reasoning` | Yes | No: it answers `reasoning_unavailable` |
| `models.load`, `models.inspect`, `models.remove`, `models.download`, `system.plan` | Yes | No: they answer `provider_model` |
| The chat's KV cache, idle-time cards written by the model | Yes | No: nothing is loaded here |

A model that calls a function it was not given (such as a suggested tool it did not load first) is refused by some providers. The Engine then answers that call itself, as it does for an installed model, so the turn goes on.

What a provider sends with a call besides its name and arguments is kept with the call (`provider_data` in the call's `<tool_call>` JSON): its call id and its own fields, such as the `thought_signature` that Gemini 3 needs back in the next request. They are sent back only to the provider that sent them. Another provider gets the call's name and arguments under the Engine's own id, and an installed model's prompt never shows them. The server chat and the terminal (`cluaiz run`, `cluaiz agent`) read streamed calls with the same code (`api/src/providers/stream.rs`), which accepts numbered calls in pieces (OpenAI, Groq) and parallel calls sent whole with their own ids (Gemini).

Errors from the provider have `"type": "provider_error"` (HTTP 502 for a whole answer) and say the cause in plain words: a refused key, an unknown model, a rate limit; a full context is `exceed_context_size_error`.

## Embeddings, speech and transcription

| Route | Sent to the provider | What comes back |
|---|---|---|
| `/v1/embeddings` | `{base_url}/embeddings` with `input`, `encoding_format`, `dimensions`, `user`, and `input_type` (`query` or `passage`) and `truncate` when sent: NVIDIA's and Voyage's query/document models need `input_type` | The provider's answer, with `model` set to the full name |
| `/v1/audio/speech` | `{base_url}/audio/speech` with `input`, `voice`, `speed`, WAV audio | The Engine's own shape: `output.audio_data` as `data:audio/wav;base64,…`; with `stream`, one `speech.audio.delta` event and the `done` event |
| `/v1/audio/transcriptions` | `{base_url}/audio/transcriptions` as a file upload, with `language`, `prompt`, `temperature` | The Engine's own shape: `output.text`, and `output.segments` with `timestamps: true`; with `stream`, the `done` event |

For transcription, the audio is read as for an installed model: `input_source` or `file` may be a data URI, a URL or an allowed local file, in any format the Engine decodes. It is sent to the provider as one-channel WAV.

Each provider has its own voices, languages and limits. The Engine passes `voice` on as it is; the provider's answer says when one is not accepted, and many providers need one. A WAV whose header has no length (as streamed audio often has) gets its real sizes, so players and the Engine read it whole. Fields a provider does not take are named in the `X-Cluaiz-Ignored` response header, and `X-Cluaiz-Provider` names the provider.

Engine memory, document ingest and workspace indexes keep using an installed embedding model: vectors from different models do not mix, and these features would otherwise send every text they index to the provider. `cluaiz model set-vector` therefore takes an installed model only.

`cluaiz provider test <name> <model>` sends a request of the model's listed kind: a short text for an embedding or text-to-speech model (`--voice <name>` when the provider needs a voice), a chat message otherwise. A speech-to-text model is tested by sending audio to `/v1/audio/transcriptions`.

## Not yet

- Request formats other than OpenAI Chat Completions and Anthropic Messages: the Gemini API's own format, the OpenAI Responses API, and Bedrock's Converse and InvokeModel APIs. Each of these platforms also offers one of the two formats above, which the Engine uses.
- AWS credentials from an EC2 instance role or a container (the instance metadata service): use the variables, the credentials file or the AWS CLI.
- Managed identities for Azure: use a service principal or the Azure CLI.
- A settings page in the App, and `type`, `auth` and `headers` in the CEP method `providers.set` (they are set in providers.json or with `cluaiz provider add`).
- Thinking text of earlier answers without calls is not sent back to the provider (the thinking of an answer with calls is, with its signature).
