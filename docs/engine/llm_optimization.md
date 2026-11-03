# Load and Speed Settings (`settings.json` → `optimization`)

These settings control how a GGUF model is loaded and run by llama.cpp. They are the
`optimization` section of `settings.json` (`OptimizationControl` in
`engine/core/src/settings.rs`). Change them with `cluaiz config set optimization.<field> <value>`
or CEP `config.set`.

Most fields are empty by default. Empty means the planner (`engine/plan`) decides from this
machine's free memory, or llama.cpp's own default applies. A value you set wins over the planner.

## Placement

| Field | Default | Meaning |
|---|---|---|
| `n_gpu_layers` | planner | Layers on the GPU. `0` = CPU only, `-1` = all layers, `N` = exactly N |
| `n_cpu_moe` | planner | llama.cpp `--n-cpu-moe`: the routed experts of the first N blocks stay in RAM, the rest of those blocks stays on the GPU |
| `offload_kqv` | planner | llama.cpp `offload_kqv`: `false` keeps the whole KV cache in RAM |
| `op_offload` | llama.cpp default | llama.cpp `op_offload`: large batches of weights held in RAM are copied to the GPU to compute there |
| `custom_vram_buffer_gb` | auto | VRAM kept free. Auto: 1.0 GB, lowered down to 0.5 GB when that lets the model fit on the GPU |
| `custom_ram_buffer_gb` | auto | RAM kept free. Auto: 1.0 GB, lowered down to 0.5 GB when that lets the model fit in RAM |
| `use_mmap` | `true` | Memory-map the GGUF file |

### When a load does not fit

What a model really takes on this GPU is measured after each load and kept in
`config/vram_margins.json`; the next plan for the same model and settings uses it instead of the
fixed buffer. If llama.cpp still cannot allocate a planned GPU placement (most often the KV cache or
a compute buffer), the Engine records the miss and plans once more with the larger margin: fewer GPU
layers, a smaller context or a smaller KV type. The plan report then says "load retry". When the
GPU layers or `n_cpu_moe` are set by hand there is no retry: the error is shown as it is.

## Context and KV cache

| Field | Default | Meaning |
|---|---|---|
| `custom_context_window` | auto | Context size in tokens. Empty = sized by free memory, `0` = the model's native maximum, `N` = exactly N |
| `kv_cache_quantization` | `Auto` | `Kv16` (F16), `Kv8` (Q8_0), `Kv4` (Q4_0). `Auto` = F16 |
| `flash_attention` | `Auto` | `On`, `Off`, `Auto` |
| `context_shifting` | `Auto` | When a chat outgrows the context: `Off` (llama.cpp default: an error, or the answer stops at the limit); `Minimal`, `Standard`, `Aggressive`, `Extreme` drop the oldest whole turns and shift the KV cache, freeing at least 5 / 10 / 25 / 50 % each time; `Auto` frees what is needed |

## Batches and threads

| Field | Default | Meaning |
|---|---|---|
| `batch_size` | 512 | Tokens per batch |
| `ubatch_size` | 512 | Physical micro-batch size |
| `n_threads` | physical cores | CPU threads for generation |
| `n_threads_batch` | same as `n_threads` | CPU threads for prompt processing |

## Thinking

| Field | Default | Meaning |
|---|---|---|
| `think_mode` | `Auto` | Thinking for reasoning models: `On`, `Off`, `Auto` |
| `reasoning_effort` | `auto` | Thinking budget for chats that send no `reasoning_effort`: `auto`, `low`, `medium`, `high`, `max` or a number of tokens. Not a load setting: changing it does not reload the model |
| `thinking_presets` | built in | Share of the room and maximum tokens for Low / Medium / High |
| `reasoning_budget_message` | built in | Text written before the end-of-thinking tag when the budget runs out |

## ONNX models

ONNX models (TTS, STT, embeddings) do not use these fields. Their precision is fixed by the model
file, and they run on CUDA when the ONNX GPU pack is installed and there is room, otherwise on the
CPU.

## See also

- What the planner chose on this machine: `cluaiz show <model>`, CEP `system.plan`.
- KV cache reuse: [prefix_caching_architecture.md](prefix_caching_architecture.md).
