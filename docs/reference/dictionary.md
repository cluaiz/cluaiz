# System Dictionary

Terms used across the Cluaiz Engine docs.

---

## Engine terms

| Term | Definition |
|---|---|
| **`cluaiz`** | The one program: CLI, terminal agent, HTTP server (`cluaiz serve`) and model runtimes. |
| **EngineHost** | The part that loads and unloads models, keeps the KV cache and shares memory between models (`engine/runtime`). |
| **Planner** | Decides how a model is placed on this machine: GPU layers, context size, KV cache type (`engine/plan`). |
| **Hardware profile** | The saved hardware probe, `hardware_profile.json` ([details](../engine/system_control.md)). |
| **GPU pack** | The llama.cpp backend libraries for CUDA 12, CUDA 13 or Vulkan, installed next to the executable. |
| **KV cache** | The attention state of the tokens already processed. Reused across turns and saved to disk per chat ([details](../engine/prefix_caching_architecture.md)). |
| **Chat Map** | One card per message (keywords, entities, tools, files), searched with BM25 to recall older messages (`context/src/memory/`). |
| **Note** | A fact the model proposes and the user confirms. Statuses: `proposed`, `confirmed`, `moved` (to the project's `.agents/memory/`), and others. |
| **Compaction** | Replacing old turns with a summary when the context is full. |
| **`.agents/` folder** | A project's agent files: `AGENTS.md`, `rules/`, `workflows/`, `skills/`, `memory/`. |
| **Managed prompt** | An Engine prompt text kept as a file and changeable with CEP `prompts.*` (`engine/core/prompts/`). |
| **Tool card** | The short description of an installed tool shown to the model; `load_tool` gives the full one. |

## CEP terms

| Term | Definition |
|---|---|
| **CEP** | Cluaiz Engine Protocol: JSON-RPC 2.0 methods named `group.verb`, over `POST /v1/cep`, `/mcp` and the C library (`cep_call` / `cep_free`). |
| **Principal** | Who a CEP call runs as (`owner`, `mcp`): its scopes (read, write, exec, admin) and the folders (roots) it may touch. |
| **Roots** | The folders a principal may read or write. Every path is made canonical and checked against them. |
| **Approval** | The owner's yes for one call (`approvals.approve`): bound to the principal, method and params, single use, 5 minutes. |
| **Task** | A long CEP call (`audio.run`, `models.download`) that returns a `task_id`; read it with `tasks.get` / `tasks.result`. |

## Settings files

| File | Reference |
|---|---|
| `settings.json` | [engine configuration](engine-configuration.md), [load and speed settings](../engine/llm_optimization.md) |
| `permission.json` | [permission.json](../engine/permission.md) |
| `hardware_profile.json` | [hardware profile](../engine/system_control.md) |

## Hardware terms

| Term | Definition |
|---|---|
| **AVX2 / AVX-512 / NEON** | CPU vector instructions used by the CPU backend |
| **CUDA** | NVIDIA GPU backend |
| **Vulkan** | GPU backend for AMD, Intel and NVIDIA |
| **Metal** | Apple GPU backend, built into the macOS build |
| **NVML** | NVIDIA's library for reading GPU memory; the hardware probe uses it |
| **Flash Attention** | An attention algorithm that needs less memory (`optimization.flash_attention`) |
| **MoE expert offload** | Keeping the routed experts of the first N blocks in RAM (`optimization.n_cpu_moe`, llama.cpp `--n-cpu-moe`) |

## Runtime limits

| Limit | Value | Where |
|---|---|---|
| CEP batch size | 32 calls | `cep/src/dispatch.rs` |
| CEP params depth | 64 levels | `cep/src/dispatch.rs` |
| Approval lifetime | 5 minutes | `cep/src/approval.rs` |
| API keys | at most 5 | `security/src/permission_schema.rs` |
| Default API port | 8000 (the owner's setting in `permission.json`) | `security/src/permission_schema.rs` |
| Saved chat KV caches | 5 GB, 30 days (defaults) | `settings.json` → `chat_memory` |
