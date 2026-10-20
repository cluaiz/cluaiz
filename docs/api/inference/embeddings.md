# Vector Embeddings API (`/v1/embeddings`)

`POST /v1/embeddings`

Generates dense floating-point vector representations for text documents, search queries, and images. Backed by ONNX execution graphs with automatic mean pooling, tokenizer attention masking, and L2 normalization for semantic search and RAG retrieval pipelines.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/embeddings`
* **Content-Type:** `application/json`
* **Authentication:** `Authorization: Bearer <session-token>` (Optional header: `x-caller: user`)
* **Output Format:** Normalized F32 floating point vector arrays (`data[].embedding`)
* **Request Body Limit:** `4 * ceil(media.limits.max_input_bytes / 3) + 2 MiB` for inline JSON, refreshed from current settings per request. Oversized bodies return HTTP413 `request_too_large`; source/decoded byte limits and native resource checks remain separate.

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Formats | Description |
|:---|:---|:---:|:---|:---|:---|
| **`input`** | String, Array of Strings, or Array of Objects | **Yes** | — | Non-empty string, string array, or image object array | The data to vectorize. For text, pass a string or array of strings. For images, pass `[{"type": "image", "source": "<uri-or-path>"}]`. |
| **`model`** | String | No | Active slot model | Installed embedding model ID (e.g. `bge_m3-unknown-onnx-fp32`, `fashion_clip-unknown-onnx-fp32`) | Exact model ID to execute. |
| **`encoding_format`** | String | No | `"float"` | `"float"` | Numerical encoding format for vectors. Non-float encodings are rejected. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` forever) | Session memory retention policy in minutes. |

---

## 🧠 Supported Input Modalities & Models

### 1. Text Embeddings (`bge_m3-unknown-onnx-fp32`)
* **Output Dimensions:** 1024 normalized dimensions.
* **Input Syntax:** String or Array of Strings:
  ```json
  {
    "model": "bge_m3-unknown-onnx-fp32",
    "input": [
      "Cluaiz high-performance local AI runtime",
      "Semantic similarity search in vector database"
    ]
  }
  ```

### 2. Image Embeddings (`fashion_clip-unknown-onnx-fp32`)
* **Output Dimensions:** 512 normalized dimensions.
* **Input Syntax:** Array containing image source object (Base64 data URI, HTTP URL, or local path):
  ```json
  {
    "model": "fashion_clip-unknown-onnx-fp32",
    "input": [
      {
        "type": "image",
        "source": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
      }
    ]
  }
  ```

---

## 💻 Code Examples

### 1. cURL (Text Embeddings)

```bash
curl -X POST http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "model": "bge_m3-unknown-onnx-fp32",
    "input": [
      "First paragraph for semantic retrieval.",
      "Second sentence for cosine similarity comparison."
    ]
  }'
```

### 2. cURL (Image Embedding Vector)

```bash
curl -X POST http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "model": "fashion_clip-unknown-onnx-fp32",
    "input": [
      {
        "type": "image",
        "source": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
      }
    ]
  }'
```

### 3. Python (Batch Text Embeddings)

```python
import requests

headers = {
    "Content-Type": "application/json",
    "Authorization": "Bearer <session-token>"
}

payload = {
    "model": "bge_m3-unknown-onnx-fp32",
    "input": [
        "First document paragraph for semantic search.",
        "Second query string to compute cosine similarity."
    ]
}

response = requests.post("http://localhost:8000/v1/embeddings", json=payload, headers=headers)
data = response.json()

for item in data.get("data", []):
    vec = item["embedding"]
    print(f"Index {item['index']} | Dimension: {len(vec)} | Sample: {vec[:3]}")
```

---

## 📤 Response Schema & Fields

### Success Response (`HTTP 200`)

```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "index": 0,
      "embedding": [
        -0.07441563,
        0.039529443,
        -0.035253406,
        0.012948211,
        -0.058192341
      ]
    },
    {
      "object": "embedding",
      "index": 1,
      "embedding": [
        0.01824123,
        -0.04519284,
        0.07129481,
        -0.02194819,
        0.00941829
      ]
    }
  ],
  "model": "bge_m3-unknown-onnx-fp32",
  "usage": {
    "prompt_tokens": 14,
    "total_tokens": 14
  }
}
```

### Response Field Reference

* **`object`**: Object type (`"list"`).
* **`data[].object`**: Item type (`"embedding"`).
* **`data[].index`**: Zero-based item index matching input order.
* **`data[].embedding`**: Finite normalized float array (L2 normalized vector).
* **`usage.prompt_tokens`**: Total tokens consumed by the tokenizer attention mask.
