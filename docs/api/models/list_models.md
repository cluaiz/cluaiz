---
method: "GET"
path: "/v1/models"
standard: "OpenAI"
category: "models"
summary: "List locally installed models in the OpenAI list format"
params: []
response: |
  {
    "object": "list",
    "data": []
  }
errors:
  - status: 401
    type: "unauthenticated"
    when: "API authentication is enabled and the Bearer token is missing or invalid"
---

# List installed models

`GET /v1/models` returns the Engine's installed model inventory. It reads the same
registry as [`models.list`](../cep/models/models/models.list.md); it does not list
downloadable repositories or load model weights.

Send `Authorization: Bearer <token>` when API authentication is enabled. There is
no request body or category filter on this route. An empty inventory returns
`{"object":"list","data":[]}` with HTTP 200.

## Response fields

| Field | Meaning |
|---|---|
| `object` | `"list"` |
| `data[]` | One entry per installed registry model |
| `data[].id` | Exact ID accepted by requests that select an installed model |
| `data[].object` | `"model"` |
| `data[].created` | Unix timestamp generated when the list is requested; not the model's download date |
| `data[].owned_by` | `"cluaiz"` |
| `data[].root` | Same installed ID |
| `data[].parent` | `null` |
| `data[].permission` | Empty array; not a list of the caller's CEP scopes |
| `data[].category` | Registry category, such as chat, embedding, vision, TTS or STT |
| `data[].context_window` | Package metadata as a string; may be unknown |
| `data[].package_capabilities` | File-backed input declarations, projector candidates, ONNX graph metadata and diagnostics; may be `null` |
| `data[].task_support` | ONNX task/attachment support resolved from the package and available loaded-session evidence; `null` for GGUF |

Package declarations and a registry entry are not proof that inference has run.
Missing package folders or primary files are removed from the inventory during
registry refresh. The selected request still validates its graph, task and helper
assets, and checks current memory availability.

## Examples

### cURL

```bash
curl -s "$CLUAIZ_URL/v1/models" -H "Authorization: Bearer $TOKEN"
```

### JavaScript

```javascript
const response = await fetch(`${engineUrl}/v1/models`, {
  headers: { Authorization: `Bearer ${token}` },
});
if (!response.ok) throw new Error(`Models request failed: ${response.status}`);
const inventory = await response.json();
for (const model of inventory.data) console.log(model.id, model.category);
```

## Other model operations

[`models.list`](../cep/models/models/models.list.md) returns the fuller registry
view, including `format`, files, supported tasks and `experimental`, and accepts
an optional category. Load, inspect, remove and download through
[`models.*`](../cep/models/README.md).

`GET /api/tags` provides the existing Ollama inventory shape
`{"models":[{"name","model","modified_at","size","details"}]}`. It uses the
same inventory; it does not include the full CEP registry entry. Its current
`modified_at` value is a fixed compatibility value, not a measured install time.
