# `POST /v1/ingest/file` API Specification

Uploads and processes documents for RAG (Retrieval-Augmented Generation) chunking.

---

## 📡 HTTP Request

```http
POST /v1/ingest/file
Content-Type: multipart/form-data
```

### Form Fields:
* `file`: Binary file payload (PDF, TXT, MD).

---

## 📡 Response Schema

```json
{
  "status": "ingested",
  "filename": "document.pdf",
  "chunks_created": 42,
  "vectorized": true
}
```
