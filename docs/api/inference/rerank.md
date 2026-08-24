# Semantic Search Reranking API

`POST /v1/rerank`

Cross-Encoder document scoring and semantic search re-ranking endpoint. Scores query-document pairs with full bidirectional cross-attention to compute precise relevance scores.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/rerank`
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`query`** | String | **Yes** | — | The search query string to rank candidate documents against. |
| **`documents`** | Array of Strings | **Yes** | — | List of candidate text documents to score and rank. |
| **`model`** | String | No | `"bge-reranker-v2-m3"` | Cross-encoder reranker model identifier. |
| **`top_n`** | Integer | No | `null` (All) | Number of top ranked results to return. If omitted, returns all scored documents. |

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/v1/rerank \
  -H "Content-Type: application/json" \
  -d '{
    "model": "bge-reranker-v2-m3",
    "query": "How does transformer attention work?",
    "documents": [
      "Attention is all you need paper introduced self-attention mechanism.",
      "Convolutional neural networks are commonly used for image classification."
    ]
  }'
```

### 2. Python

```python
import requests

payload = {
    "model": "bge-reranker-v2-m3",
    "query": "How does transformer attention work?",
    "documents": [
        "Attention is all you need paper introduced self-attention mechanism.",
        "Convolutional neural networks are commonly used for image classification."
    ]
}

res = requests.post("http://localhost:8000/v1/rerank", json=payload)
results = res.json().get("results", [])
for r in results:
    print(f"Score: {r['relevance_score']:.4f} | Text: {r['document']}")
```

---

## 📤 Response Format

```json
{
  "id": "rerank-123e4567-e89b-12d3-a456-426614174000",
  "model": "bge-reranker-v2-m3",
  "results": [
    {
      "index": 0,
      "relevance_score": 0.9982,
      "document": "Attention is all you need paper introduced self-attention mechanism."
    },
    {
      "index": 1,
      "relevance_score": 0.0121,
      "document": "Convolutional neural networks are commonly used for image classification."
    }
  ],
  "usage": {
    "total_documents": 2,
    "prompt_tokens": 34
  }
}
```
