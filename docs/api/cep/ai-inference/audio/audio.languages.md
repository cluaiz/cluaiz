---
method: "audio.languages"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "audio"
summary: "Languages declared by the selected transcription model"
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
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "audio.languages",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "data": [
        "af",
        "ar",
        "bn",
        "de",
        "en",
        "es",
        "fr",
        "hi",
        "ja",
        "zh"
      ],
      "model": "whisper-large-v3-turbo-INT8",
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

# audio.languages

The language codes a transcription (STT) model accepts as `language`, and as candidates in `languages`, read from its package without loading it.

The model is `model`, else the active STT slot, else the first installed model that declares speech-to-text. Answers `{"object": "list", "model", "data": ["af", "ar", …]}`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"audio.languages","params":{"model":"whisper-large-v3-turbo-INT8"}}'
```

Leave out the `Authorization` header when API authentication is off.
