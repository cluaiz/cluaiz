---
method: "DELETE"
path: "/api/delete"
group: "Models"
category: "models"
summary: "Removes an installed model, as cluaiz rm does; answers 200 with an empty body"
params:
  - name: "model"
    type: "String"
    req: true
    default: ""
    allowed: "An installed model id (`cluaiz list`)"
    desc: "The model to remove. Ollama's older `name` field is read when `model` is empty"
request_payload: |
  {
    "model": "smollm-135m-gguf-Q2_K"
  }
errors:
  - status: 400
    type: "error"
    when: "The body is missing or is not JSON, `model` is empty, or `model` looks like a path (`..`, `/` or a backslash)"
  - status: 401
    type: "unauthenticated"
    when: "API authentication is on and no valid token was sent"
  - status: 404
    type: "error"
    when: "The model is not installed; the body is `{\"error\": \"model 'X' not found\"}`"
  - status: 409
    type: "error"
    when: "The model is a provider's (`<provider>/<model>` in providers.json); this route works on installed models only"
  - status: 500
    type: "error"
    when: "Removing the model failed"
---

# Delete Model API

`DELETE /api/delete` (Ollama) · CEP `models.remove`

Removes an installed model with the same code as CEP `models.remove` and `cluaiz rm`: its folder and its registry entry. If it is the loaded chat model, it is unloaded first. A model that is not installed is `404`.

---

## `DELETE /api/delete`

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`model`** | String | **Yes** | - | An installed model id, e.g. `"smollm-135m-gguf-Q2_K"`. Ollama's older `name` field is read when `model` is empty |

On success the answer is `200` with an empty body. Errors use Ollama's shape, `{"error": "..."}`.

```bash
curl -X DELETE http://localhost:8000/api/delete \
  -H "Content-Type: application/json" \
  -d '{"model": "smollm-135m-gguf-Q2_K"}'
```

The same call again, after the model is gone:

```json
{"error":"model 'smollm-135m-gguf-Q2_K' not found"}
```

A model id is not a path: `..`, `/` and `\` are refused with `400` and nothing is deleted.

Contract test: `api/tests/ollama_delete_contract.rs` (200 with an empty body, then 404, the `name` field, 400 for bad bodies, 401 without a token).

Add `-H "Authorization: Bearer <token>"` when API authentication is on.

---

## CEP `models.remove`

The same removal as a CEP method: see [`models.remove`](../cep/models/models/models.remove.md).
