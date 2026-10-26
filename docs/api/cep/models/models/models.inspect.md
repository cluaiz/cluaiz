---
method: "models.inspect"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Raw GGUF metadata and tensor table of an installed model"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "file"
    type: "string | null"
    req: false
    desc: "A file inside the model's folder, relative to it (default: the primary weight file)"
    example: null
  - name: "model"
    type: "string"
    req: true
    desc: "An installed model id (from `models.list`)"
    example: "bonsai-27b-gguf-q1_0"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.inspect",
    "params": {
      "model": "bonsai-27b-gguf-q1_0"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "file": "C:/Users/me/.cluaiz/models/chat/bonsai-27b-gguf-q1_0/Bonsai-27B-Q1_0.gguf",
      "int_arrays": {
        "qwen35.rope.dimension_sections": [
          11,
          11,
          10,
          0
        ]
      },
      "metadata": {
        "general.architecture": "qwen35",
        "general.name": "Bonsai 27B",
        "qwen35.block_count": 64,
        "qwen35.context_length": 262144
      },
      "model": "bonsai-27b-gguf-q1_0",
      "speech_languages": [],
      "tensors": [
        {
          "dims": [
            5120,
            248320
          ],
          "ggml_type": 41,
          "name": "output.weight",
          "offset": 0,
          "size_bytes": 178790400
        }
      ]
    }
  }
errors:
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or its weight file is missing"
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty or looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The model's folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32602
    type: "invalid_params"
    when: "`file` leaves the model's folder"
  - code: -32010
    type: "gguf_probe_failed"
    when: "The file is not a readable GGUF"
---

# models.inspect

The raw header of an installed GGUF file: every metadata key, integer arrays, speech languages, and the tensor table. Reads the header only, not the weights.

## What it does

1. Finds the model; with `file`, picks that file inside the model's folder (it may not leave the folder), else the primary weight file.
2. Checks the path against the caller's roots.
3. Reads the GGUF header and answers it. A file that is not GGUF (an ONNX graph) fails with `-32010` `gguf_probe_failed`.

## Result

| Field | Meaning |
|---|---|
| `model`, `file` | The model id and the file that was read |
| `metadata` | Every `key: value` of the header (`general.architecture`, `<arch>.context_length`, tokenizer keys…) |
| `int_arrays` | Integer array keys (for example rope sections) |
| `speech_languages` | Languages a speech codec declares, when it has any |
| `tensors[]` | `name`, `dims`, `ggml_type`, `offset`, `size_bytes` of every tensor |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.inspect","params":{"model":"bonsai-27b-gguf-q1_0"}}'
```

Leave out the `Authorization` header when API authentication is off.
