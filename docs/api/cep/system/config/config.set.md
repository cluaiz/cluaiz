---
method: "config.set"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "config"
summary: "Changes the given fields of one settings section; answers with the section as saved"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "patch"
    type: "any"
    req: true
    desc: "Only the fields to change"
    example: {"cross_chat":"ask"}
  - name: "section"
    type: "\"gguf\" | \"onnx\" | \"optimization\" | \"context\" | \"storage\" | \"kv\" | \"engine\""
    req: true
    desc: "The section to change"
    example: "context"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "config.set",
    "params": {
      "patch": {
        "cross_chat": "ask"
      },
      "section": "context"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "context": {
        "cross_chat": "ask"
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "An unknown field or an invalid value in `patch` (section engine ignores unknown fields instead)"
  - code: -32010
    type: "settings_not_saved"
    when: "settings.json or permission.json could not be written"
---

# config.set

Changes the given fields of one settings section and answers with the section as saved. Only the fields in `patch` change; the rest of the section stays as it is.

## Per section

| Section | Accepts | On a bad field |
|---|---|---|
| `gguf` | `hardware_and_execution` (`n_gpu_layers`: -1 = auto, 0 = CPU, N; `n_ctx`: -1 = max, 0 = auto, N; `no_mmap`; `batch_size`; `ubatch_size`), `samplers` (the whole sampler set: `temp`, `top_p`, `top_k`, `min_p`, `presence_penalty`, `frequency_penalty`, `repeat_penalty`, `seed`; `null` = the model's default), `user_moved_flags.think_mode` | `-32602` |
| `onnx` | Fields of the onnx section | `-32602` |
| `optimization` | Fields of the optimization section, merged over the saved ones | `-32602` |
| `context` | Fields of the context policy; percentages are whole numbers 0–100, `recent_turns_min` at most 50. The first write of this section copies `settings.json` to `settings.json.bak-<date>-<time>` | `-32602` |
| `storage` | `cleanup_policy` | `-32602` |
| `kv` | `enabled`, `max_disk_gb` (0 or more), `max_age_days`; the limits are applied at once (old saved KV is deleted) | `-32602` |
| `engine` | `api_auth`, `active_slots`, `chat_models`, `vector_models`, `exec` (each replaced whole) | Other fields are ignored, not refused |

Settings are read at the next request that needs them; a model that is loaded keeps its load-time values until it is loaded again.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"config.set","params":{"section":"context","patch":{"compact_at_pct":80}}}'
```

Leave out the `Authorization` header when API authentication is off.
