# Vector Embeddings API

`POST /v1/embeddings`

Generates dense floating-point vector representations for text inputs, supporting both `.onnx` and `.gguf` embedding models with automatic hardware acceleration, mean pooling, and L2 normalization.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/embeddings`
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`input`** | String or Array of Strings | **Yes** | — | The input text or batch of texts to vectorize. |
| **`model`** | String | No | `"auto"` | Model ID. Defaults to the active embedding model loaded in the slot. |
| **`encoding_format`** | String | No | `"float"` | Format for vector numbers (`"float"` or `"base64"`). |

---

## 💻 Code Examples

### 1. cURL (Single Text)

```bash
curl -X POST http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "auto",
    "input": "Cluaiz Engine high-performance local AI runtime."
  }'
```

### 2. Python (Batch Embedding)

```python
from openai import OpenAI

client = OpenAI(base_url="http://localhost:8000/v1", api_key="not-needed")

response = client.embeddings.create(
    model="auto",
    input=[
        "First document paragraph for semantic search.",
        "Second query string to compute cosine similarity."
    ]
)

vector_1 = response.data[0].embedding
vector_2 = response.data[1].embedding
print(f"Dimension: {len(vector_1)}")
```

---

## 📤 Response Format

```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "index": 0,
      "embedding": [
        0.0245123,
        -0.0134521,
        0.0891234,
        -0.0456123,
        0.0012345
      ]
    }
  ],
  "model": "bge-large-en-v1.5",
  "usage": {
    "prompt_tokens": 12,
    "total_tokens": 12
  }
}
```
