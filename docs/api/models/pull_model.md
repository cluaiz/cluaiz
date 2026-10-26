---
method: "POST"
path: "/api/pull"
standard: "Ollama"
category: "models"
summary: "Downloads and registers a model, as cluaiz pull does; streams one JSON line per step"
params:
  - name: "model"
    type: "String"
    req: true
    default: ""
    allowed: "Catalog id, Hugging Face owner/name[:QUANT], hf:// or https URL"
    desc: "The model to download"
  - name: "stream"
    type: "Boolean"
    req: false
    default: "true"
    allowed: "true, false"
    desc: "One NDJSON line per step; false answers once at the end"
  - name: "insecure"
    type: "Boolean"
    req: false
    default: "false"
    allowed: "true, false"
    desc: "Accepted for Ollama clients; no effect (downloads use HTTPS)"
request_payload: |
  {
    "model": "QuantFactory/SmolLM-135M-GGUF:Q2_K",
    "stream": false
  }
response: |
  {
    "status": "success",
    "model": "smollm-135m-gguf-Q2_K",
    "path": "C:/Users/me/.cluaiz/models/chat/smollm-135m-gguf-Q2_K/SmolLM-135M.Q2_K.gguf",
    "downloaded": true
  }
streaming: "`stream: true` (default): application/x-ndjson, one `{\"status\"}` line per step, `total` / `completed` bytes while downloading, then the result line or `{\"error\"}`"
errors:
  - status: 400
    type: "error"
    when: "`model` is empty, over 512 characters, or has control characters"
  - status: 401
    type: "unauthenticated"
    when: "API authentication is on and no valid token was sent"
  - status: 409
    type: "error"
    when: "The same model is being pulled already (stream: false)"
  - status: 500
    type: "error"
    when: "Resolving or downloading failed (stream: false)"
---

# Download / Pull Model API

`POST /api/pull` (Ollama) · CEP `models.download`

Downloads and registers a model with the same code as `cluaiz pull`. A catalog id, or a Hugging Face repository `owner/name`, optionally with `:QUANT` (an `hf://` or https URL works too). With a `:QUANT` tag that variant is downloaded, else the repository's first variant; nothing asks a question.

---

## 📌 `POST /api/pull`

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`model`** | String | **Yes** | — | Catalog id or Hugging Face repo, e.g. `"QuantFactory/SmolLM-135M-GGUF:Q2_K"` |
| `stream` | Boolean | No | `true` | One JSON line per step (`application/x-ndjson`); `false` answers once at the end |
| `insecure` | Boolean | No | `false` | Accepted for Ollama clients; no effect (downloads use HTTPS) |

Example of the streamed lines (the byte numbers are illustrative):

```json
{"status":"scanning QuantFactory/SmolLM-135M-GGUF"}
{"status":"tag :Q2_K matched SmolLM-135M.Q2_K.gguf (0.08 GB)"}
{"status":"pulling manifest"}
{"status":"downloading","total":88195040,"completed":1048576}
{"status":"success","model":"smollm-135m-gguf-Q2_K","path":"…/models/chat/smollm-135m-gguf-Q2_K/SmolLM-135M.Q2_K.gguf","downloaded":true}
```

`total` / `completed` are bytes of the file being downloaded, sent at most once per percent. `downloaded: false` means the model was installed already (nothing was downloaded). A failure ends the stream with `{"error": "…"}`. Closing the connection stops the download.

With `"stream": false`: the last line as the answer; `400` for an empty `model`, `409` while the same model is being pulled, `500` with `{"error"}` when the download fails.

```bash
curl -X POST http://localhost:8000/api/pull \
  -H "Content-Type: application/json" \
  -d '{"model": "QuantFactory/SmolLM-135M-GGUF:Q2_K"}'
```

---

## 📌 CEP `models.download`

The same download as a task: `{"model": "…"}` answers at once with `{"task": {"task_id", …}}`. The steps are task events, the bytes are the task's `progress` (`unit: "bytes"`), and `tasks.result` gives the final object above. `tasks.cancel` stops it (`-32008`). A second download of a model that is still downloading fails with `-32005` (`data.type: "pull_running"`); a failed download is `-32010` (`download_failed`).

```bash
curl -X POST http://localhost:8000/v1/cep -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.download","params":{"model":"QuantFactory/SmolLM-135M-GGUF:Q2_K"}}'
```

Add `-H "Authorization: Bearer <token>"` to either call when API authentication is on.
