---
method: "POST"
path: "/api/show"
group: "Models"
category: "models"
summary: "An installed model's details and GGUF header in Ollama's show shape; nothing is loaded"
params:
  - name: "model"
    type: "String"
    req: true
    default: ""
    allowed: "An installed model id (`cluaiz list`)"
    desc: "The model to describe. Ollama's older `name` field is read when `model` is empty"
  - name: "verbose"
    type: "Boolean"
    req: false
    default: "false"
    allowed: "true, false"
    desc: "Accepted for Ollama clients; the tensor list is not sent yet"
request_payload: |
  {
    "model": "olmoe-1b-7b-0125-instruct-gguf-Q4_K_M"
  }
response: |
  {
    "modelfile": "",
    "parameters": "",
    "template": "{{ bos_token }}{% for message in messages %}...",
    "details": {
      "parent_model": "",
      "format": "gguf",
      "family": "olmoe",
      "families": ["olmoe"],
      "parameter_size": "6.9B",
      "quantization_level": "Q4_K_M"
    },
    "model_info": {
      "general.architecture": "olmoe",
      "general.file_type": 15,
      "general.parameter_count": 6919161856,
      "olmoe.context_length": 4096,
      "olmoe.embedding_length": 2048,
      "tokenizer.ggml.tokens": []
    },
    "modified_at": "2026-11-16T03:47:08Z"
  }
errors:
  - status: 400
    type: "error"
    when: "No body (`missing request body`), a body that is not JSON, or no `model` and no `name` (`model is required`)"
  - status: 401
    type: "unauthenticated"
    when: "API authentication is on and no valid token was sent"
  - status: 404
    type: "error"
    when: "`model 'NAME' not found`: no installed model with this id, or its weight file is missing (a provider's model is not installed, so it is 404 too)"
  - status: 500
    type: "error"
    when: "The GGUF header could not be read"
---

# Show Model API

`POST /api/show` (Ollama) · CEP [`models.inspect`](../cep/models/models/models.inspect.md)

An installed model's details from the registry and its GGUF header, in the shape Ollama's own
`/api/show` answers (`ShowResponse` in Ollama's `api/types.go`). The model is not loaded: only the
file header is read, the same data CEP `models.inspect` returns.

---

## `POST /api/show`

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`model`** | String | **Yes** | - | An installed model id; Ollama's older `name` is read when `model` is empty |
| `verbose` | Boolean | No | `false` | Accepted; the tensor list is not sent yet |

Other Ollama fields (`system`, `template`, `options`) are accepted and not used.

| Reply field | Where it comes from |
|:---|:---|
| `details.format`, `details.family`, `details.families` | the registry's format and architecture |
| `details.parameter_size` | the registry's value; when it says `Unknown`, the weights counted from the GGUF tensor table (every tensor's element count, summed over all parts of a split model), written as Ollama writes it (`6.9B`, `125.08M`). `/api/tags` and `/api/ps` send the same value |
| `details.quantization_level` | the quantization in the weight file's name (`Q4_K_M`) |
| `model_info` | every GGUF header key, with `general.parameter_count` added; `general.name` and `tokenizer.chat_template` are left out, as Ollama leaves them out. Numbers and booleans are JSON numbers and booleans; long arrays (the vocabulary) are `[]`, as Ollama sends them |
| `template` | the model's Jinja chat template (`tokenizer.chat_template`), as Ollama sends it for a GGUF model with its own template |
| `modified_at` | the change time of the weight file |
| `modelfile`, `parameters` | empty: Cluaiz has no Modelfile |

A model that is not GGUF (an ONNX speech model) gets its `details` and an empty `model_info`.

```bash
curl -X POST http://localhost:8000/api/show \
  -H "Content-Type: application/json" \
  -d '{"model": "olmoe-1b-7b-0125-instruct-gguf-Q4_K_M"}'
```

An unknown model:

```json
{"error":"model 'no-such-model' not found"}
```

The examples on this page were recorded on 2026-11-22 against an Engine built from source, with
OLMoE-1B-7B-0125-Instruct Q4_K_M installed. A value that the header stores as text but that looks
like a number (for example a version string `"2"`) comes back as a number: the Engine's GGUF reader
keeps every value as text, so the original type is not known.

Add `-H "Authorization: Bearer <token>"` when API authentication is on.
