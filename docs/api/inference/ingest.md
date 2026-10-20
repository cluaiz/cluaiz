# Native Document Extraction & OCR API (`/v1/ingest`)

`POST /v1/ingest` (alias: `POST /v1/ingest/file`)

Native high-performance document parsing and OCR extraction service. Powered by embedded PDFium and Tesseract 5.2.0 OCR engines. It extracts clean UTF-8 text and structured chunks from digital PDFs, scanned/image-only PDFs, Microsoft Word (.docx), plain text files, and images without requiring an LLM or GPU model.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/ingest` (or `/v1/ingest/file`)
* **Content-Type:** `application/json`
* **Authentication:** `Authorization: Bearer <session-token>` (Optional header: `x-caller: user`)
* **Request Body Limit:** `4 * ceil(media.limits.max_input_bytes / 3) + 2 MiB` for inline JSON, refreshed from current settings per request. Oversized bodies return HTTP413 `request_too_large`; source/decoded byte limits remain separate.

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Formats | Description |
|:---|:---|:---:|:---|:---|:---|
| **`source`** | String or Array of Strings | **Yes** | — | Local file path, file:// URI, HTTP(S) URL, or Base64 Data URI | Single document source or ordered array of documents to process in batch. |
| **`output_controls`** | Object | No | `{"return_text": true}` | `{"return_text": boolean}` | Controls returned fields. `return_text: true` returns chunks and extracted text. `return_text: false` preserves metadata and recognition confidence while removing raw text. |
| **`chunking_strategy`** | Object | No | `{"type": "semantic", "max_chunk_size": 512, "overlap": 0}` | `type: "semantic"`, `overlap: 0` | Configures document chunk segmentation. `max_chunk_size` is a positive UTF-8 byte boundary limit (not tokens). Nonzero overlap is currently unsupported. |

---

## 🛑 Banned & Unsupported Parameters

The following legacy or cloud-oriented parameters are **explicitly rejected with HTTP 400 Bad Request**:
* ❌ **`namespace`**: Ingestion is a pure extraction service; it does not persist into arbitrary vector database namespaces. Use `/v1/embeddings` separately.
* ❌ **`model` / `vision_model`**: Native extraction uses embedded C++ PDFium and Tesseract libraries; external vision model names are rejected.
* ❌ **`return_embeddings: true`**: Ingestion does not compute vector embeddings inline. Chunks returned from this endpoint can be fed directly to `POST /v1/embeddings`.

---

## 📄 Supported Document Formats

| Format | Extension / MIME | Extraction Mechanism | Capabilities & Notes |
|:---|:---|:---|:---|
| **Digital PDF** | `.pdf` (`application/pdf`) | Native PDFium Parser | Extracts selectable page text, Unicode symbols, and table layouts. |
| **Scanned PDF** | `.pdf` (`application/pdf`) | Native Tesseract 5.2.0 OCR | Automatically triggered on image-only/empty pages to extract visual text. |
| **Word Document** | `.docx` (`application/vnd.openxmlformats...`) | OOXML Body Parser | Parses main-body XML paragraph streams and table cells. (Headers/footers remain omitted). |
| **Plain Text & Code** | `.txt`, `.md`, `.rs`, `.ts`, `.py`, `.json` | UTF-8 Stream Reader | Direct lossless character extraction and UTF-8 validation. |
| **Raster Images** | `.png`, `.jpg`, `.jpeg`, `.webp`, `.bmp` | Tesseract OCR Pipeline | Performs preprocessing, deskew, and OCR recognition with confidence score. |

---

## 💻 Code Examples

### 1. cURL (Base64 Data URI)

```bash
curl -X POST http://localhost:8000/v1/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "source": "data:text/plain;base64,SGVsbG8gQ2x1YWl6IEVuZ2luZSEgVGhpcyBpcyBhIGZ1bGwtdGV4dCBkb2N1bWVudCBleHRyYWN0aW9uIHRlc3Qu",
    "output_controls": {
      "return_text": true
    },
    "chunking_strategy": {
      "type": "semantic",
      "max_chunk_size": 512,
      "overlap": 0
    }
  }'
```

### 2. cURL (Local Authorized File Path - Desktop / Tauri)

```bash
curl -X POST http://localhost:8000/v1/ingest \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -H "x-caller: user" \
  -d '{
    "source": "C:/Users/Aryan/Documents/annual_report.pdf",
    "output_controls": {
      "return_text": true
    }
  }'
```

### 3. Python (Requests)

```python
import requests
import base64

# Read PDF and encode as Data URI
with open("contract.pdf", "rb") as f:
    pdf_bytes = f.read()
b64_data = "data:application/pdf;base64," + base64.b64encode(pdf_bytes).decode("utf-8")

headers = {
    "Content-Type": "application/json",
    "Authorization": "Bearer <session-token>"
}

payload = {
    "source": b64_data,
    "output_controls": {
        "return_text": True
    }
}

response = requests.post("http://localhost:8000/v1/ingest", json=payload, headers=headers)
data = response.json()

if data.get("status") == "success":
    for source_res in data.get("processed_sources", []):
        print(f"MIME: {source_res['mime_type']} | Chunks: {source_res['chunks_count']}")
        for chunk in source_res.get("chunks", []):
            print(f"- {chunk[:100]}...")
```

---

## 📤 Response Schema & Fields

### Success Response (`HTTP 200`)

```json
{
  "status": "success",
  "total_sources": 1,
  "processed_sources": [
    {
      "source": "data:text/plain;base64,...",
      "status": "ingested",
      "mime_type": "text/plain",
      "byte_size": 81,
      "chunk_size": 512,
      "chunks_count": 1,
      "chunks": [
        "Hello Cluaiz Engine! This is a full-text document extraction test."
      ],
      "parts": [
        {
          "page": 1,
          "method": "utf8",
          "text": "Hello Cluaiz Engine! This is a full-text document extraction test.",
          "omissions": [],
          "warnings": [],
          "ocr_confidence": null
        }
      ]
    }
  ]
}
```

### Response Field Reference

| Field | Type | Description |
|:---|:---|:---|
| **`status`** | String | Overall request status: `"success"`, `"partial"` (some sources or pages omitted), or `"error"`. |
| **`total_sources`** | Integer | Total number of documents requested in the batch. |
| **`processed_sources[].status`** | String | Individual document status: `"ingested"`, `"partial"`, or `"error"`. |
| **`processed_sources[].mime_type`**| String | Detected MIME type (e.g. `"application/pdf"`, `"text/plain"`). |
| **`processed_sources[].parts[].method`** | String | Extraction method: `"utf8"`, `"pdf_text"`, `"docx_xml"`, `"native_ocr"`, `"pdf_ocr"`, or `"pdf_text_and_ocr"`. |
| **`processed_sources[].parts[].page`** | Integer / null | 1-based physical page number (for PDFs); `null` for plain text/Word. |
| **`processed_sources[].parts[].omissions`** | Array of Strings | Specific page sections that could not be parsed (e.g. complex vector drawings, encrypted sections). |
| **`processed_sources[].parts[].ocr_confidence`**| Integer / null | Tesseract mean recognition confidence score (0 to 100); `null` if digital text. |

---

## 🔗 Chat Integration (`file_url`)

When using `POST /v1/chat/completions`, attaching a document via `{ "type": "file_url", "file_url": { "url": "<source>" } }` automatically routes through this exact native extraction service:
* Digital and scanned PDFs are extracted into pure text context.
* **No Vision Projector Required:** Because text is extracted natively by the engine, standard text-only GGUF models (e.g. `llama_3.2_instruct-3b-gguf-q4_k_m`) can read and summarize documents without any vision hardware overhead.
