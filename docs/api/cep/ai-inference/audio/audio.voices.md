---
method: "audio.voices"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "audio"
summary: "Voice inventory and speech capabilities without loading weights"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "model"
    type: "string | null"
    req: false
    desc: "An installed model id (default: the active slot, else the first installed model for the task)"
    example: "kokoro-82m-onnx"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "audio.voices",
    "params": {
      "model": "kokoro-82m-onnx"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "capabilities": {
        "coherent_input": false,
        "language_control": "explicit",
        "languages": [
          "en-US",
          "en-GB",
          "hi"
        ],
        "voice_mode": "preset"
      },
      "data": [
        {
          "default": true,
          "id": "default",
          "language": null
        },
        {
          "default": false,
          "id": "af_heart",
          "language": "en-US"
        },
        {
          "default": false,
          "id": "hf_alpha",
          "language": "hi"
        }
      ],
      "model": "kokoro-82m-onnx",
      "object": "list"
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or no installed model for the task"
  - code: -32602
    type: "model_contract_error"
    when: "The model's package does not match its declared task (missing voices, config or assets)"
  - code: -32002
    type: "forbidden"
    when: "The model or the source file is outside the caller's roots (`data.needs: \"roots\"`)"
---

# audio.voices

The voices a speech (TTS) model offers, and what it can do, read from its package without loading weights. Use the ids as `voice` in `/v1/audio/speech` or as `parameters.voice_id` in [`audio.run`](audio.run.md).

## What it does

1. Picks the model: `model`, else the active TTS slot, else the first installed model that declares text-to-speech.
2. Checks its folder against the caller's roots.
3. Lists `default` first (the Engine picks a voice for the text's language), then the package's voices with the language each name declares.

## Result

| Field | Meaning |
|---|---|
| `model` | The model that answered |
| `data[]` | `id`, `language` (`null` when unknown), `default` (`true` only for `default`) |
| `capabilities` | `languages`, `language_control` (`explicit`: `language` is honoured; `auto`: taken from the text), `voice_mode` (`preset`, `embedded`, `generated`, `reference_source`, `reference_profile`), `coherent_input` |

A GGUF speech model answers only `default` plus its capabilities; any other `voice` for it is a reference audio source.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"audio.voices","params":{"model":"kokoro-82m-onnx"}}'
```

Leave out the `Authorization` header when API authentication is off.
