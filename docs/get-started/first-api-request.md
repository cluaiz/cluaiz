# Your first API request

In ten minutes: a running server, a small model, one answer, one streamed answer, and an API key. Every
output below was recorded on Windows 11 with Engine 0.1.0 on 2026-11-21 (an isolated home on port 8011);
long JSON is shortened where marked `…`.

You need Cluaiz installed ([Quickstart](quickstart.md)) and `curl`.

## 1. Download a small model

```bash
cluaiz pull unsloth/Qwen3-0.6B-GGUF:Q4_K_M
cluaiz list
```

```text
ID                        CATEGORY    FORMAT        SIZE
qwen3-0.6b-gguf-Q4_K_M    chat        gguf       0.40 GB
```

The **installed ID** (`qwen3-0.6b-gguf-Q4_K_M`) is what the API calls `model`. It is not the Hugging Face
repository name you pulled. See [Model downloading](model-downloading.md) for choosing a model and a
quantization.

## 2. Start the server

```bash
cluaiz serve
```

It listens on port 8000 (`cluaiz permission port <n>` changes it). In a second terminal:

```bash
curl http://localhost:8000/health
```

```json
{"channel":"pre-release","engine":"Cluaiz Engine","message":"Cluaiz Engine is alive and operational.","status":"alive","version":"0.1.0"}
```

## 3. Ask for an answer

The chat route is OpenAI's: `POST /v1/chat/completions`.

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen3-0.6b-gguf-Q4_K_M",
    "messages": [{"role": "user", "content": "Name the capital of France in one word."}],
    "think_mode": "off",
    "max_tokens": 20
  }'
```

```json
{"choices":[{"finish_reason":"stop","index":0,"message":{"content":"**Paris**","role":"assistant"}}],"context_report":{…}, …}
```

- `think_mode: "off"` asks a thinking model (Qwen3 is one) to answer without thinking first. With thinking
  on, `reasoning_effort` sets how long it may think; see
  [Reasoning budget levels](../api/inference/chat_completions.md#reasoning-budget-levels-think_mode--reasoning_effort).
- `max_tokens` limits the answer, not the thinking.
- The reply has OpenAI's fields plus Cluaiz's own (`context_report`: how the context was planned). OpenAI
  client libraries ignore fields they do not know.

PowerShell has its own `curl` alias; use `curl.exe` with the same arguments, or `Invoke-RestMethod`.

## 4. Stream the answer

Add `"stream": true` and the answer arrives as Server-Sent Events, one piece of text at a time:

```bash
curl -N http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "qwen3-0.6b-gguf-Q4_K_M", "messages": [{"role": "user", "content": "Say hi."}],
       "think_mode": "off", "max_tokens": 10, "stream": true, "stream_options": {"include_usage": true}}'
```

```text
data: {"choices":[],…,"usage":{…}}
data: {"choices":[{"delta":{"content":"Hello"}}],…,"object":"chat.completion.chunk"}
data: {"choices":[{"delta":{"content":"!"}}],…}
data: {"choices":[{"delta":{"content":" How"}}],…}
…
data: [DONE]
```

`stream_options.include_usage: true` adds a chunk with the token counts, as in OpenAI's API. To stop a
running answer, call the CEP method
[`chat.cancel`](../api/cep/ai-inference/chat/chat.cancel.md).

## 5. When it goes wrong

A model name that is not installed:

```json
{"error":{"code":"model_not_found","message":"Requested model 'no-such-model' was not found","type":"invalid_request_error"}}
```

HTTP 400. Check the ID with `cluaiz list`. More messages and fixes: [Troubleshooting](troubleshooting.md).

## 6. Protect the server with a key

By default the server answers every request. Before other machines or people can reach it, turn on keys:

```bash
cluaiz token create
```

```text
  ✨ New API Token Created Successfully!
  🔑 Token: sk-cluaiz-…
  🛡️ Auth Guard: Enabled
```

**Restart `cluaiz serve`.** The command writes `permission.json`; a server that is already running keeps
the setting it started with (recorded: the running server still answered without a key, HTTP 200; after a
restart it refused, HTTP 401). The CEP method [`tokens.create`](../api/cep/system/tokens/tokens.create.md)
adds a key to the running server at once, but does not turn the guard on by itself.

```bash
curl http://localhost:8000/v1/models
# {"error":"Unauthorized","message":"API Authentication is required. Provide a valid Bearer token in the Authorization header."}   HTTP 401

curl http://localhost:8000/v1/models -H "Authorization: Bearer sk-cluaiz-…"
# HTTP 200
```

`/health` stays open (HTTP 200 without a key) so uptime checks keep working. Details:
[Network access and API tokens](../engine/permission.md#network-access-and-api-tokens).

## Next

- From an OpenAI client library: set its base URL to `http://localhost:8000/v1`; which fields an
  installed model honours: [OpenAI compatibility](../api/inference/openai_compatibility.md).
- Everything the Engine does, one protocol: [CEP](../api/cep/README.md).
- All HTTP routes: [API overview](../api/README.md).
