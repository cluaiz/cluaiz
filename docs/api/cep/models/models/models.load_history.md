---
method: "models.load_history"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "models"
summary: "This machine's chat-model loads, refusals, failures and unloads, oldest first: placement, VRAM planned and taken, attempts, time (the newest 500 are kept)"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "from_seq"
    type: "integer | null"
    req: false
    desc: "First seq to return (default 1)"
    example: 1
  - name: "limit"
    type: "integer | null"
    req: false
    desc: "Most records to return (default 100, at most 500)"
    example: 20
  - name: "model"
    type: "string | null"
    req: false
    desc: "Only the records of this model id (also one removed since); all when absent"
    example: "qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "models.load_history",
    "params": {
      "from_seq": 1,
      "limit": 20,
      "model": "qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "next_from_seq": null,
      "records": [
        {
          "action": "load",
          "at": "2026-11-18T17:16:49.438Z",
          "attempts": 2,
          "cause": "requested",
          "error": "Context Init Failure",
          "file": "C:/Users/me/.cluaiz/models/chat/qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M/Qwen3-30B-A3B-Instruct-2507-Q4_K_M.gguf",
          "model": "qwen3-30b-a3b-instruct-2507-gguf-Q4_K_M",
          "ms": 106406,
          "n_cpu_moe": 48,
          "n_ctx": 16384,
          "n_gpu_layers": -1,
          "outcome": "ok",
          "placement": "MoeSplit",
          "seq": 1,
          "vram_planned_gb": 2.56
        }
      ],
      "skipped_lines": 0,
      "total": 1
    }
  }
errors: []
---

# models.load_history

Read what the Engine did with chat models on this machine: every load, refused load, failed load and unload, with the placement it chose, the VRAM it planned and took, how many llama.cpp loads it needed and how long the action took. Use it to show load times and failures in an app or the CLI, or to see why a model ran in a smaller placement than expected.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `models.load_history` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | None; reads `load_history.jsonl` in the Engine's config folder |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. See [the common CEP contract](../../README.md).

## How it works

1. Each chat-model load, refusal, failure and unload appends one JSON line to `load_history.jsonl` (beside `vram_margins.json`). Only the newest 500 records are kept; `seq` keeps counting across the trim.
2. The method reads the file, keeps the records of `model` when given, and returns those with `seq >= from_seq`, at most `limit`.
3. A line that cannot be read is skipped and counted in `skipped_lines`; it never fails the call.

A model that is already loaded with the same settings is not loaded again and writes no record. Records hold ids, numbers and error text, never chat content.

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `model` | string \| null | No | Only this model id. It may be a model removed since; no install check is made. Omitted: every model. |
| `from_seq` | integer \| null | No | First `seq` to return. Omitted: 1. |
| `limit` | integer \| null | No | Most records to return. Omitted: 100; values are kept between 1 and 500. |

Unknown top-level method parameters are rejected.

## Result

| Field | Meaning |
|---|---|
| `records` | The records, oldest first |
| `total` | Records kept for this filter |
| `skipped_lines` | Lines of the file that could not be read |
| `next_from_seq` | `from_seq` for the next page; `null` when this page reached the end |

Each record:

| Field | Meaning |
|---|---|
| `seq`, `at` | Position in the history; RFC 3339 time the action ended |
| `action` | `load` or `unload` |
| `outcome` | `ok`; `failed` (llama.cpp could not load it, the retries included); `refused` (no placement fits, nothing was loaded) |
| `cause` | Loads only. `requested`; `remeasured` (the first load measured the model and a better placement fits); `step_down` (too little VRAM was left after the load, loaded one placement smaller) |
| `model`, `file`, `file_bytes` | Installed model id, weight file, its size |
| `attempts` | llama.cpp loads for this action: more than 1 when the planned placement failed and smaller ones were tried (at most 3) |
| `placement`, `n_gpu_layers`, `n_cpu_moe`, `n_ctx` | The placement as loaded (for `failed` and `refused`, as planned) |
| `vram_planned_gb`, `vram_taken_gb` | VRAM the plan counted; free VRAM before the load minus after it. `vram_taken_gb` is absent when it could not be measured (no GPU, a load fallback, shared GPU memory) |
| `ms` | Wall time of the action. A first load reads the weights from disk and is much slower than a load of a file the system has cached |
| `error` | Why the first attempt failed (the retry may still have loaded it), or why the action failed |

Optional fields are left out when they do not apply. Readers should ignore fields they do not know.
