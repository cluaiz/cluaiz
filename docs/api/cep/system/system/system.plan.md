---
method: "system.plan"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Plans an installed model without loading its weights or changing saved settings"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "batch_tokens"
    type: "integer | null"
    req: false
    desc: "Logical batch size in tokens for the plan (llama.cpp `-b`; default: the saved setting)"
    example: null
  - name: "model"
    type: "string"
    req: true
    desc: "An installed model id (from `models.list`)"
    example: "installed-model"
  - name: "model_path"
    type: "string | null"
    req: false
    desc: "Plan this weight file instead of the model's primary file (it must pass the caller's roots)"
    example: null
  - name: "n_ctx"
    type: "integer | null"
    req: false
    desc: "Context size: -1 = the model's maximum, 0 = automatic, N = N tokens (default: the saved setting)"
    example: null
  - name: "ram_buffer_gb"
    type: "number | null"
    req: false
    desc: "RAM to keep free, in GB (default: the planner's rule)"
    example: null
  - name: "vram_buffer_gb"
    type: "number | null"
    req: false
    desc: "VRAM to keep free, in GB (default: the planner's rule)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.plan",
    "params": {
      "model": "installed-model"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "active_user_settings": {
        "flash_attention": "Auto",
        "kv_cache_quantization": "Auto"
      },
      "candidates": [],
      "execution_plan": {
        "batch_size": 512,
        "context_reason": "Auto: floor 4096 tokens, KV Kv8; model does not fit on one device",
        "context_window": 4096,
        "decision_reason": "GPU + RAM split: 16 / 64 blocks + output head on GPU (1.57 GB), 48 blocks in RAM",
        "gpu_layers": 16,
        "kv_cache_location": "vram + ram (next to each layer)",
        "kv_cache_type": "Kv8",
        "model_memory_breakdown": {
          "kv_cache_ram_gb": 0.4,
          "kv_cache_vram_gb": 0.13,
          "ram_gb": 2.96,
          "vram_gb": 1.57
        },
        "n_gpu_layers": 17
      },
      "hardware_info": {},
      "model_info": {
        "architecture": "qwen35",
        "context_window": 262144,
        "inputs": [
          {
            "available": true,
            "type": "text"
          }
        ],
        "model_id": "bonsai-27b-gguf-q1_0",
        "model_size_gb": 3.54,
        "outputs": [
          "text"
        ],
        "total_layers": 64
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`model` is empty, `n_ctx` is below -1, `batch_tokens` is 0, or a buffer is negative"
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, and no usable `model_path`"
  - code: -32002
    type: "forbidden"
    when: "`model_path` is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "gguf_probe_failed"
    when: "The GGUF header could not be read"
  - code: -32010
    type: "onnx_probe_failed"
    when: "The ONNX package could not be inspected"
---

# system.plan

The execution plan an installed model would get on this machine, without loading its weights and without changing saved settings: how many layers go on the GPU, the context window, the KV cache type and where it lives, and the memory each part takes. Use it to show a user what will happen before they load a model, or to compare settings.

## What it does

1. Finds the model in the registry (`model`), or plans the file at `model_path` (which must pass the caller's roots).
2. Starts from the saved settings (`config` sections `gguf` / `optimization`) and applies only the overrides sent here (`n_ctx`, `batch_tokens`, `vram_buffer_gb`, `ram_buffer_gb`). The saved settings stay as they are.
3. For a GGUF, reads the file header (geometry: layers, heads, context length) and runs the same planner `models.load` and chat use. For an ONNX package, reports its inputs, outputs, task routes and missing assets instead.
4. Answers the plan. No weights are read and nothing is loaded.

## Rules

- `n_ctx`: `-1` = the model's maximum, `0` = automatic (the planner picks), `N` = exactly N tokens.
- `batch_tokens` must be at least 1; the buffers must be 0 or more.
- Live free memory is read when planning, so a plan can differ from minute to minute while other programs use the GPU.

## How the planner chooses

The planner works out every placement that could run the model and keeps the fastest one that fits. Generating a token is limited by memory bandwidth, so weights in VRAM beat weights in RAM, and RAM beats streaming from disk.

```mermaid
flowchart TD
    Start["Weights + KV cache + logits for the chosen context"] --> GPU{"Fit in usable VRAM?"}
    GPU -->|yes| Full["Full GPU: every layer and the KV cache in VRAM"]
    GPU -->|no| Split["GPU + RAM split: as many layers as fit on the GPU, the rest in RAM; KV next to each layer"]
    Split --> MoE{"Mixture of experts?"}
    MoE -->|yes| Experts["Expert weights of some layers stay in RAM (n_cpu_moe)"]
    Split --> CPU["CPU only: everything in RAM"]
    Full --> Winner["The feasible candidate with the most work on the GPU wins"]
    Split --> Winner
    Experts --> Winner
    CPU --> Winner
```

"Usable VRAM" is the GPU's free memory now, minus the VRAM buffer (`vram_buffer_gb`, or the planner's rule). With `n_ctx: 0` the planner picks the context window too: it starts at a floor (4096 tokens) and grows it while the placement still fits.

## Result

| Field | Meaning |
|---|---|
| `model_info` | `model_id`, `architecture`, `context_window` (the model's own limit), `total_layers`, `model_size_gb`, `inputs[]` (`type`, `available`), `outputs[]` |
| `execution_plan` | The winner: `context_window` and `context_reason`, `gpu_layers` / `n_gpu_layers` / `ram_layers`, `n_cpu_moe`, `placement_mode`, `batch_size` / `ubatch_size`, `kv_cache_type`, `kv_cache_location`, `offload_kqv`, `use_mmap`, `disk_stream_gb`, `decision_reason`, `model_memory_breakdown` (`vram_gb`, `ram_gb`, `kv_cache_vram_gb`, `kv_cache_ram_gb`, `logits_buffer_gb`, `remaining_free_vram_gb`, `remaining_free_ram_gb`) |
| `candidates[]` | Every placement considered: `name` (`Full GPU`, `GPU + RAM split`, `CPU only`), `tier`, `feasible`, `n_gpu_layers`, `vram_required_gb`, `ram_required_gb`, `kv_gpu_gb`, `kv_ram_gb`, `missing_gb` (how much memory was short), `reason` |
| `active_user_settings` | The settings the plan used (saved settings with this call's overrides) |
| `hardware_info` | `cpu`, `gpu`, `ram` as the planner saw them |

A real candidate list for a 27B model with 1-bit weights on a 4 GB GPU: `Full GPU` not feasible ("weights 3.37 + KV 0.53 + logits 0.47 = 4.37 GB vs usable VRAM 1.62 GB"), `GPU + RAM split` feasible (16 of 64 blocks plus the output head on the GPU), `CPU only` feasible. The split wins.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.plan","params":{"model":"bonsai-27b-gguf-q1_0","n_ctx":8192}}'
```

Leave out the `Authorization` header when API authentication is off.
