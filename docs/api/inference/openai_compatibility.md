<!-- name: OpenAI compatibility
description: Which OpenAI routes and fields the Engine serves, which differ, and how to point an OpenAI client library at Cluaiz. -->

# OpenAI compatibility

Cluaiz serves OpenAI's chat, models, embeddings and audio routes, so an OpenAI client library works by
pointing its base URL at the Engine. This page lists which request fields an installed (llama.cpp) model
honours, which it refuses, and what Cluaiz adds. Everything marked "recorded" was checked in run 32
(`test/cloud/runs/2026-11-21-run32-openai-compat-probe/`, Engine 0.1.0, Windows, Qwen3-0.6B) or, after the fix, run 34
(`test/cloud/runs/2026-11-21-run34-openai-stop-and-limit/`).

## Connect a client

```python
from openai import OpenAI

client = OpenAI(base_url="http://localhost:8000/v1", api_key="sk-cluaiz-...")  # any text when keys are off
reply = client.chat.completions.create(
    model="qwen3-0.6b-gguf-Q4_K_M",           # the installed ID from `cluaiz list`
    messages=[{"role": "user", "content": "Name the capital of France."}],
)
print(reply.choices[0].message.content)
```

The `model` is the installed ID that `cluaiz list` prints, not the Hugging Face repository name. The
Python example above was not run in run 32 (the requests there used `curl`).

## Routes

| OpenAI route | Cluaiz | Page |
|---|---|---|
| `POST /v1/chat/completions` | yes, streaming and not | [Chat completions](chat_completions.md) |
| `GET /v1/models` | yes | [Models](../models/list_models.md) |
| `POST /v1/embeddings` | yes | [Embeddings](embeddings.md) |
| `POST /v1/audio/speech` | yes | [Speech](audio_speech.md) |
| `POST /v1/audio/transcriptions` | yes | [Transcriptions](audio_transcriptions.md) |
| Responses, Assistants, Files, Batches, Images | no | |

## Chat request fields on an installed model

![OpenAI chat fields an installed model honours, including logit_bias, and the ones it refuses with HTTP 400: n above 1, logprobs and top_logprobs](../../assets/api/openai-fields.png)

| Field | Honoured | Notes |
|---|---|---|
| `model`, `messages`, `stream` | yes | |
| `stream_options.include_usage` | yes | the usage chunk at the end, as OpenAI sends it |
| `max_tokens` | yes | limits the answer; thinking has its own budget (`reasoning_effort`) |
| `temperature`, `top_p`, `frequency_penalty`, `presence_penalty`, `seed` | yes | passed to the sampler |
| `response_format` | yes | JSON or a JSON schema, enforced by a grammar; with tools, one grammar covers the answer and the calls (run 68) |
| `tools`, `tool_choice` | yes | `tool_choice`: `auto`, `none` (no Engine tools, no call asked for), `required` (a call is forced) or a named function (only that function, call forced); tools also work with image and audio messages (run 68) |
| `reasoning_effort` | yes | `low`, `medium`, `high`, plus `auto`, `max` or a number of tokens |
| `stop` | yes (recorded) | a string or an array; the answer ends before the first match, `finish_reason: "stop"` (run 34) |
| `max_completion_tokens` | yes (recorded) | the answer limit when `max_tokens` is not sent; `max_tokens` wins when both are (run 34) |
| `n` | **no** | `n: 1` works; above 1 the request is refused with HTTP 400 (one choice per request) |
| `logprobs`, `top_logprobs` | **no** | `logprobs: false` and `top_logprobs: 0` work; asking for them is refused with HTTP 400 |
| `logit_bias` | yes (GGUF) | token ids as strings, -100 to 100; llama.cpp's logit-bias sampler applies them. A wrong key or value, an id past the vocabulary, or a non-GGUF model: HTTP 400 naming `logit_bias`. `{}` works |
| unknown fields | ignored | no error is returned |

## What Cluaiz adds

Extra request fields (all optional): `think_mode`, `reasoning_budget_message`, `top_k`, `min_p`,
`repetition_penalty`, `grammar`, `chat_template_kwargs`, `session_id`, `kv_reuse`, `max_steps`,
`tool_search`, `context_policy`, `llama` (load settings). The reply carries OpenAI's fields plus
`context_report` (how the context was planned); OpenAI libraries ignore fields they do not know.
Details of each: [Chat completions](chat_completions.md).

An OpenAI client library sends these through `extra_body`; they are merged into the request body as
they are (the same way vLLM documents its extra parameters):

```python
reply = client.chat.completions.create(
    model="qwen3-0.6b-gguf-Q4_K_M",
    messages=[{"role": "user", "content": "Summarise this file."}],
    max_tokens=512,
    extra_body={
        "think_mode": "off",          # no thinking for this request
        "top_k": 40,
        "llama": {"n_ctx": 8192},     # load settings: here the context size
        "keep_alive": 0,              # unload the model after the answer
    },
)
```

OpenAI's API has no field for the context size or the GPU layers; on Cluaiz they go in `llama`
(`n_ctx`, `n_gpu_layers`, `cache_type_k`, ...). Unset, the planner chooses them from free memory. A
request with different load settings reloads the model. (The `extra_body` example was not run; the fields are the ones `/v1/chat/completions` reads, `api/src/handlers/chat/types.rs`.)

## Known differences

The fields marked **no** above are recorded gaps in the Engine, not choices. Until they are fixed,
an installed model refuses them instead of answering without them: HTTP 400 with OpenAI's error
shape, `code: "unsupported_parameter"` and `param` naming the field (when streaming: one SSE error
chunk, then `[DONE]`). Nothing is loaded for a refused request.

```json
{"error":{"code":"unsupported_parameter","message":"n=2 is not supported on installed models: one answer per request (send n=1 or leave it out)","param":"n","type":"invalid_request_error"}}
```

That reply was recorded on 2026-11-22 (OLMoE-1B-7B, `"n":2`). A provider model is not checked:
`logit_bias` is forwarded to the provider; `n` and `logprobs` are not forwarded.

### logit_bias

The keys are token ids of the model's own tokenizer, so the same word has different ids on different
models, and one word can have more than one id (with and without a leading space). Recorded on
2026-11-23 with Qwen3.5-0.8B Q4_K_M, `temperature: 0`, "What is the capital of France? Answer with
one word.":

| `logit_bias` | Answer |
|---|---|
| none | `Paris` |
| `{"57590": -100}` (`Paris`) | `Paris` (the model picks id 11751, the same word with a space) |
| `{"57590": -100, "11751": -100}` | `Lyon` |
| `{"10025": 100}` (`Blue`) | `Blue` repeated up to `max_tokens` |
| `{"Paris": -100}`, `{"248320": 5}`, `{"57590": -150}` | HTTP 400, `param: "logit_bias"` |
| `{}` | `Paris` |

```json
{"error":{"code":"invalid_value","message":"logit_bias token 248320 is not in this model's vocabulary (248320 tokens, ids 0 to 248319)","param":"logit_bias","type":"invalid_request_error"}}
```
