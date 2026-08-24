# Document AI & Spatial Vision OCR Ingestion API

> [!NOTE]
> `/v1/ingest` is a native Cluaiz Engine extension for hardware-accelerated local document AI, OCR, and vector ingestion.

`POST /v1/ingest`

Specialized Document AI and Spatial Vision OCR extraction endpoint. Parses, ingests, and vectorizes documents, PDFs, scanned pages, financial reports, complex tables, and high-resolution images into structured text and embeddings.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/ingest`
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`source`** | String or Array | **Yes** | — | File path (`C:/...`), Web URL, or Base64 Data URI of the target document/image. |
| **`namespace`** | String | No | `"default"` | Vector database namespace partition for storage. |
| **`vision_model`** | String | No | `"got-ocr-2.0"` | OCR / Document vision extraction model identifier. |
| **`embedding_model`** | String | No | `"bge-m3"` | Embedding model identifier for vectorization. |
| **`output_controls`** | Object | No | `{"return_text": true, "return_embeddings": true}` | Granular flags controlling returned metadata payload. |

---

## 💻 Code Examples

### 1. cURL (Local PDF / Document)

```bash
curl -X POST http://localhost:8000/v1/ingest \
  -H "Content-Type: application/json" \
  -d '{
    "source": "C:/Users/Aryan/Documents/annual_report.pdf",
    "namespace": "financial_docs",
    "vision_model": "got-ocr-2.0",
    "embedding_model": "bge-m3"
  }'
```

### 2. Python (Document Extraction)

```python
import requests

payload = {
    "source": "https://example.com/research_paper.pdf",
    "namespace": "research_papers",
    "output_controls": {
        "return_text": True,
        "return_embeddings": False
    }
}

response = requests.post("http://localhost:8000/v1/ingest", json=payload)
data = response.json()
for chunk in data.get("data", []):
    print(f"[{chunk['index']}] {chunk['text']}\n")
```

---

## 📤 Response Format

```json
{
  "object": "list",
  "namespace": "financial_docs",
  "data": [
    {
      "source_url": "C:/Users/Aryan/Documents/annual_report.pdf",
      "total_file_chunks": 2,
      "chunks": [
        {
          "index": 0,
          "text": "Executive Summary: The company achieved 45% revenue growth...",
          "embedding": [0.0124, -0.0345, 0.0892]
        },
        {
          "index": 1,
          "text": "Table 1.1: Financial Performance Breakdown (in Millions)...",
          "embedding": [0.0451, 0.0112, -0.0567]
        }
      ]
    }
  ],
  "usage": {
    "total_files_processed": 1,
    "total_chunks": 2
  }
}
```
