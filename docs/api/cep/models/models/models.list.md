---
method: "models.list"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Installed registry entries, optionally filtered by category"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "category"
    type: "\"chat\" | \"embedding\" | \"vision-embedding\" | \"vision\" | \"tts\" | \"stt\" | null"
    req: false
    desc: "Only models of this category; all when absent"
    example: "stt"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.list",
    "params": {
      "category": "stt"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": [
      {
        "category": "stt",
        "experimental": false,
        "extra_files": [
          "config.json",
          "tokenizer.json"
        ],
        "files": [
          {
            "is_primary": true,
            "name": "decoder_model_int8.onnx",
            "size_bytes": 439684851
          },
          {
            "is_primary": false,
            "name": "encoder_model_int8.onnx",
            "size_bytes": 644822094
          }
        ],
        "format": "onnx",
        "huggingface_repo": "onnx-community/whisper-large-v3-turbo",
        "id": "whisper-large-v3-turbo-INT8",
        "local_dir": "C:/Users/me/.cluaiz/models/stt/whisper-large-v3-turbo-INT8",
        "metadata": {
          "architecture": "onnx-generic",
          "backend_type": "onnx"
        },
        "size": 1950984246,
        "supported_tasks": [
          "speech_to_text",
          "automatic-speech-recognition"
        ]
      }
    ]
  }
errors: []
---

# models.list

The installed models from the Engine's model registry (`model_registry.json` in the home), sorted by id, optionally only one category. The same registry feeds `GET /v1/models`.

## What it does

1. Loads the registry. An entry whose weight file was deleted by hand is dropped (and the registry saved), so the list only names models that can run.
2. Filters by `category` when given: `chat`, `embedding`, `vision-embedding`, `vision`, `tts`, `stt`.
3. Answers each entry with `format` (`gguf` / `onnx`), `size` (bytes of all its files) and `experimental` added.

## Entry fields

| Field | Meaning |
|---|---|
| `id` | The exact id to send as `model` everywhere (chat, embeddings, audio, ingest, `models.*`) |
| `category`, `format`, `size` | What it is, its file format, bytes on disk |
| `huggingface_repo`, `local_dir`, `files[]`, `extra_files[]` | Where it came from and where it is; `files[].is_primary` marks the weight file that is loaded |
| `supported_tasks[]` | The tasks it declares (`chat-completion`, `speech_to_text`, `feature-extraction`…) |
| `metadata` | Architecture, parameters, quantization, context window, ONNX capabilities… |
| `experimental` | `true` for a speech family whose runtime support is unfinished: it runs when named, but is never picked automatically |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.list","params":{"category":"chat"}}'
```

Leave out the `Authorization` header when API authentication is off.
