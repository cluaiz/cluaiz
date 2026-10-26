---
method: "models.download"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first"
scope: "write"
execution: "task"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "model"
    type: "string"
    req: true
    desc: "A catalog id, or a Hugging Face repo `owner/name` (optionally `:QUANT`, an `hf://` or https URL)"
    example: "owner/name-GGUF:Q4_K_M"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.download",
    "params": {
      "model": "owner/name-GGUF:Q4_K_M"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "task": {
        "method": "models.download",
        "status": "working",
        "task_id": "t_5f0c…"
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty, over 512 characters, or has control characters"
  - code: -32005
    type: "pull_running"
    when: "This model is being downloaded already (from the task's result)"
  - code: -32010
    type: "download_failed"
    when: "Resolving or downloading failed: no such repo or variant, network, disk (from the task's result)"
  - code: -32008
    type: "cancelled"
    when: "`tasks.cancel` stopped the download (from the task's result)"
---

# models.download

Downloads and registers a model, as a task. It runs the same code as `cluaiz pull`, without questions: a `:QUANT` tag picks that variant, otherwise the repository's first variant is used. `POST /api/pull` (Ollama) runs the same download.

## What `model` can be

| Form | Example |
|---|---|
| A catalog id | `qwen3-4b` (the online catalog names its Hugging Face repo) |
| A Hugging Face repo | `QuantFactory/SmolLM-135M-GGUF` |
| A repo with a variant | `QuantFactory/SmolLM-135M-GGUF:Q2_K` |
| `hf://` or `https://huggingface.co/...` | the same repo as a URL |

## What it does

1. Answers at once with a task handle (`{"task": {"task_id", …}}`).
2. Resolves the id: the catalog, then the repository's variants, then the variant matching the tag (or the first). Each step is a task event: `{"status": "scanning <repo>"}`, `{"status": "tag :Q2_K matched …"}`, `{"status": "pulling manifest"}`.
3. Downloads every file of the variant into `models/<category>/<id>/` in the home. The bytes are the task's `progress` (`unit: "bytes"`, at most once per percent). A partial file keeps a `.part` name and resumes on the next download.
4. Registers the model. The task's result is `{"status": "success", "model", "path", "downloaded": true}`.

A model that is installed already is not downloaded again: the result has `"downloaded": false`. A second download of the same model while one runs fails with `-32005` `pull_running`. [`tasks.cancel`](../../system/tasks/tasks.cancel.md) stops it; nothing is registered.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"models.download","params":{"model":"QuantFactory/SmolLM-135M-GGUF:Q2_K"}}'
```

Leave out the `Authorization` header when API authentication is off.

Then poll the task:

```json
{"jsonrpc":"2.0","id":2,"method":"tasks.get","params":{"task_id":"t_e225…","after":0}}
{"jsonrpc":"2.0","id":3,"method":"tasks.result","params":{"task_id":"t_e225…","wait_ms":30000}}
```
