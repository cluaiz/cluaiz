---
method: "POST"
path: "/v1/embeddings"
standard: "OpenAI"
category: "ai-inference"
summary: "Vectors for text (GGUF or ONNX) or for an image (ONNX image graph)"
params:
  - name: "input"
    type: "String, Array of Strings, or Array of Objects"
    req: true
    default: "—"
    allowed: "Non-empty string, string array, or image object array"
    desc: "The data to vectorize. For text, pass a string or array of strings. For images, pass `[{\"type\": \"image\", \"source\": \"<uri-or-path>\"}]`."
  - name: "model"
    type: "String"
    req: false
    default: "Active slot model"
    allowed: "Installed compatible embedding model ID"
    desc: "Exact selected ONNX or GGUF model; GGUF accepts text only."
  - name: "encoding_format"
    type: "String"
    req: false
    default: "\"float\""
    allowed: "`\"float\"`"
    desc: "Numerical encoding format for vectors. Non-float encodings are rejected."
  - name: "keep_alive"
    type: "Integer"
    req: false
    default: "null"
    allowed: "Minutes (e.g. `5`, `0` to unload, `-1` forever)"
    desc: "Session memory retention policy in minutes."
  - name: "user"
    type: "String"
    req: false
    default: "—"
    allowed: "Caller identifier"
    desc: "Accepted for compatibility; does not create a namespace or user-specific vector store."
request_payload: |
  {
    "model": "bge-m3-GGUF",
    "input": [
      "Cluaiz runs models locally."
    ]
  }
response: |
  {
    "object": "list",
    "data": [
      {
        "object": "embedding",
        "embedding": [
          -0.06533846,
          0.055673946,
          -0.04819885,
          -0.0075248517,
          "… 1024 numbers"
        ],
        "index": 0
      }
    ],
    "model": "bge-m3-GGUF",
    "usage": {
      "prompt_tokens": 10,
      "total_tokens": 10
    }
  }
errors:
  - status: 400
    type: "invalid_request_error"
    when: "Empty input, an unknown field, or an encoding other than float"
  - status: 400
    type: "unsupported_task"
    when: "The model does not declare an embedding task"
  - status: 400
    type: "unsupported_input"
    when: "Image or object parts sent to a GGUF model"
  - status: 401
    type: "unauthenticated"
    when: "API authentication is on and no valid token was sent"
  - status: 404
    type: "model_not_found"
    when: "The model is not installed"
  - status: 413
    type: "request_too_large"
    when: "The body (with inline images) is over the attachment limit"
  - status: 429
    type: "embedding_busy"
    when: "Another GGUF embedding request is running"
---

# Vector Embeddings API (`/v1/embeddings`)

`POST /v1/embeddings`

Returns vectors computed by the selected installed model. Text and text batches support compatible ONNX graphs or GGUF sequence-embedding models; image input requires an ONNX image graph. ONNX uses its declared output and pooling configuration; GGUF uses native mean, CLS or last-token pooling from model metadata. Both return finite L2-normalized vectors. Vector dimensions and retrieval quality depend on the selected model.

GGUF execution has its own serialized resident, preserving chat KV and speech state. Placement and threads use the existing planner/settings. The active sequence limit is the explicit context setting within the model's limit, or the configured batch size when no context is selected. Inputs beyond that limit are rejected without truncation. A second active GGUF embedding request receives HTTP429 `embedding_busy`; cancelling the caller keeps admission held until its blocking worker exits. A native operation in progress is not preempted.

Unsupported GGUF architectures, models without supported sequence pooling, rerankers and encoder-decoder models do not produce substitute vectors. Installation alone is not proof of runtime compatibility. In particular, a newer model architecture may require a separately reviewed native backend update.

## Choose the model and input

Read `models.list` with your Bearer token and copy an exact `models[].id`. A Hugging Face repository name or a guessed quantization suffix is not an installed ID. In the API Explorer, the Model dropdown writes this ID into the JSON `model` field. Reset Payload restores the shipped example; a previously saved body otherwise stays in the editor.

| Task | Model required | Request input | Result |
|:---|:---|:---|:---|
| One text | Compatible ONNX or GGUF text embedding package | `"input": "Find similar documents"` | One vector at `data[0].embedding` |
| Text batch | Same text embedding package for the whole batch | `"input": ["First chunk", "Second chunk"]` | One vector per string, in input order |
| Image | ONNX image embedding graph and its processor assets | `"input": [{"type":"image","source":"<source>"}]` | One image vector |
| Document | `ingest.run` can extract and run a selected embedding model; alternatively send its returned chunks here | `"input": ["extracted chunk"]` on this route | Text vectors; no document storage is created |

The pinned native backend now supports `gemma-embedding2`. The locally exercised EmbeddingGemma-2 Q8_0 package returns its projected 768-dimensional **text** vector; its 512-wide hidden state is not the API output. The exercised BGE packages return 1024 dimensions, and the ONNX FashionCLIP package returns 512 image dimensions. These are package examples, not a fixed width for every model. GGUF image/audio/video embeddings are not implemented, even if an installed package advertises a visual task.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/embeddings`
* **Content-Type:** `application/json`
* **Authentication:** `Authorization: Bearer <session-token>` when API authentication is on
* **Output Format:** Normalized F32 floating point vector arrays (`data[].embedding`)
* **Request Body Limit:** `4 * ceil(media.limits.max_input_bytes / 3) + 2 MiB` for inline JSON, refreshed from current settings per request. Oversized bodies return HTTP413 `request_too_large`; source/decoded byte limits and native resource checks remain separate.

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Formats | Description |
|:---|:---|:---:|:---|:---|:---|
| **`input`** | String, Array of Strings, or Array of Objects | **Yes** | — | Non-empty string, string array, or image object array | The data to vectorize. For text, pass a string or array of strings. For images, pass `[{"type": "image", "source": "<uri-or-path>"}]`. |
| **`model`** | String | No | Active slot model | Installed compatible embedding model ID | Exact selected ONNX or GGUF model; GGUF accepts text only. |
| **`encoding_format`** | String | No | `"float"` | `"float"` | Numerical encoding format for vectors. Non-float encodings are rejected. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` forever) | Session memory retention policy in minutes. |
| **`user`** | String | No | — | Caller identifier | Accepted for compatibility; does not create a namespace or user-specific vector store. |

Unknown top-level fields are rejected. There is no `dimensions` override, base64 output encoding, token-ID input, document-source input, or streaming response on this route. Keep vectors from different models/dimensions in separate indexes; equal vector lengths alone do not make their spaces compatible.

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
        "source": "data:image/png;base64,<actual-base64-png-bytes>"
      }
    ]
  }
  ```

### 3. GGUF text embeddings (EmbeddingGemma-2)

Replace the example ID with the exact installed package ID when yours differs:

```json
{
  "model": "embeddinggemma-2-GGUF",
  "input": ["How do I run a local model?", "Local inference runs on your own computer."],
  "encoding_format": "float",
  "keep_alive": 0
}
```

The engine uses the selected tokenizer and pooling/projected output. Supply the query/document formatting recommended for your model yourself; this API does not invent retrieval instruction prefixes. `keep_alive: 0` releases the resident after this request. A positive value retains it for that many minutes; `-1`/omission retains it until replacement or shutdown.

### 4. Image with optional text parts (ONNX only)

```json
{
  "model": "<installed-onnx-graph-with-image-and-text-inputs>",
  "input": [
    {"type": "text", "text": "A blue jacket"},
    {"type": "image", "source": "https://example.com/jacket.png"}
  ],
  "encoding_format": "float"
}
```

Use this mixed-input example only for a graph that actually accepts both `pixel_values` and `input_ids`. The installed FashionCLIP graph declares both inputs and image/text embedding outputs; this route selects its image vector when an image is supplied. It does not generate captions or OCR text. Use exactly one image; this is not a batch of independent image vectors. `source` can be an allowed HTTP(S) URL, an authorized server-local path, or a real image data URI. `image_url` is the chat content shape, not the embeddings shape. Audio, video and document parts are rejected on this route; document sources use `ingest.run`.

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
        "source": "data:image/png;base64,<actual-base64-png-bytes>"
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

### 4. Python (Encode a real image)

```python
import base64
import os
from pathlib import Path
import requests

source = "data:image/png;base64," + base64.b64encode(Path("jacket.png").read_bytes()).decode("ascii")
response = requests.post("http://127.0.0.1:8000/v1/embeddings",
    headers={"Authorization": "Bearer " + os.environ["CLUAIZ_TOKEN"]},
    json={"model": os.environ["CLUAIZ_IMAGE_EMBEDDING_MODEL"],
          "input": [{"type": "image", "source": source}], "keep_alive": 0})
response.raise_for_status()
print(len(response.json()["data"][0]["embedding"]))
```

Use an installed ONNX image embedding ID and a real PNG file. Placeholders and shortened Base64 strings elsewhere in this page illustrate the source shape; they are not executable image fixtures.

### 5. PDF file directly to text vectors: send this to `ingest.run`

```json
{
  "model": "embeddinggemma-2-GGUF",
  "source": "C:/documents/report.pdf",
  "output_controls": {"return_text": true, "return_embeddings": true}
}
```

The ingest endpoint performs native PDF text/OCR extraction, chunks the extracted text, and runs the selected text embedding executor. Read `processed_sources[].embeddings[].data[].embedding`. Do not send a raw PDF as an `input` string here: a string on `/v1/embeddings` is text to tokenize, not a file locator.

### 6. DOCX file directly to text vectors: send this to `ingest.run`

```json
{
  "model": "bge_m3-unknown-onnx-fp32",
  "source": "C:/documents/report.docx",
  "output_controls": {"return_text": false, "return_embeddings": true}
}
```

Choose an installed text embedding graph. Legacy supported DOC and text files use the same ingest flow. For PDFs as **images**, select an ONNX image graph on `ingest.run`; that returns one rendered-page vector per page rather than text-chunk vectors. See [all ingest params by file type](../cep/ai-inference/ingest/ingest.run.md#params-by-file-type).

The Explorer's Body Payload menu also provides separate single-text, batch, extracted-document chunks, local image, data-URI image and remote-image templates. Switching an example resolves an installed model of its declared task; it does not invent or install a model.

---

## 📤 Response Schema & Fields

### Success Response (`HTTP 200`)

The numerical arrays below are shortened illustrations, not complete model output or measured samples. Real arrays have the selected model's full dimension.

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
* **`usage.prompt_tokens`**: Actual tokenizer count from the selected backend: ONNX attention-mask tokens or native GGUF token IDs.

## Errors and resource behavior

| Condition | Response / behavior |
|:---|:---|
| Missing/invalid Bearer token when API authentication is required | HTTP 401 |
| Explicit model is not installed | HTTP 404 `model_not_found` |
| Selected package does not declare an embedding task | HTTP 400 `unsupported_task` |
| GGUF receives object parts | HTTP 400 `unsupported_input`; use text strings or an ONNX image graph |
| Empty batch/text, unsupported encoding or unknown fields | HTTP 400 |
| Input exceeds the selected GGUF sequence limit | Error; no silent truncation |
| Another GGUF embedding operation is active | HTTP 429 `embedding_busy` |
| Inline JSON exceeds the configured transport ceiling | HTTP 413 `request_too_large` |

Host/device memory admission can reject a load even when the download is small. GGUF embeddings use a separate resident from chat and speech; all still share the same physical RAM/VRAM and load-transition checks. Device placement comes from engine settings, not a `device` field in this payload. This endpoint computes vectors; saving, indexing and searching those vectors belong to your application.
