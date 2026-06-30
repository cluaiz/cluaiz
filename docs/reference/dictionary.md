# System Dictionary

Global reference for every term, type, constant, and concept across the cluaiz engine — CEL runtime, compute backend, hardware layer, configuration profiles, and API contracts.

---

## 🏗️ Core Architecture Terms

| Term | Definition |
|---|---|
| **cluaizd** | The primary database engine binary. Manages KV storage, WAL, and index operations via LMDB. |
| **cluaiz-engine** | The orchestration layer — an Axum HTTP + Tokio async server that routes CEL pipelines to inference or driver backends. |
| **cluaiz-kernel** | CPU-native inference module. Executes tensor math via SIMD (AVX512, AVX2, NEON). No GPU required. |
| **cluaiz-driver** | Hardware accelerator bridge. Dispatches GPU workloads to CUDA, Metal, or Vulkan backends dynamically. |
| **cluaiz-cli** | Terminal shell and Ratatui TUI dashboard. Communicates with the engine via Local REST / IPC Loop. |
| **CEL** | Cluaiz Execution Language — a strict, hardware-accelerated pipeline language for driving the engine. |
| **Silicon Truth** | The read-only artifact (`system_control.json`) generated at boot from physical hardware probing. Source of truth for all allocation decisions. |
| **VRAM Arbiter** | Internal subsystem inside the Master Router that enforces GPU memory boundaries and prevents OOM crashes. |
| **Conflict Manager** | Phase that cross-references `system_booster.json` requests against `system_control.json` constraints before execution. |
| **KV-Cache** | Key-Value cache for intermediate transformer attention states. Critical for context continuity and generation speed. |
| **FFI Bridge** | Foreign Function Interface boundary between the CEL VM and native C/WASM/Rust extension modules. |
| **WAL** | Write-Ahead Log — a crash-recovery mechanism that journals all mutations before committing to LMDB storage. |
| **IPC Loop** | Inter-Process Communication loop between `cluaiz-cli` and `cluaiz-engine` using local REST or named pipes. |

---

## 🧩 CEL — Primitive Types

| Type | CEL Syntax | Rust Representation | Description |
|---|---|---|---|
| `Text` | `"Hello World"` | `&str` / `String` | UTF-8 string slice |
| `Number` | `42.5` | `f64` | 64-bit float precision |
| `Bool` | `true` / `false` | `bool` | Boolean flag |
| `Vector` | `[0.1, -0.2, 0.9]` | `Vec<f32>` | High-dimensional embeddings |
| `Variable` | `$user_profile` | Dynamic KV lookup | Evaluated key reference from session scope |
| `Null` | `null` | `Option::None` | Zero-allocation unit / absence value |
| `Bytes` | `b"raw"` | `Vec<u8>` | Raw binary payload |
| `Map` | `{ key: value }` | `HashMap<String, Value>` | Key-value structure |
| `List` | `[a, b, c]` | `Vec<Value>` | Ordered sequence |

---

## ⚡ CEL — Reserved Operators

| Operator | Symbol | Description |
|---|---|---|
| Pipe | `->` | Sequential stage chaining |
| Filter | `FILTER` | Predicate-based selection over collections |
| Process | `PROCESS(text)` | Apply NLP transformation |
| Select | `SELECT` | Projection / field extraction |
| Similar | `SIMILAR` | Vector cosine similarity search |
| Time Window | `TIME_WINDOW` | Temporal range filter |
| Find | `FIND` | Exact or fuzzy key lookup |
| ForEach | `FOREACH` | Iteration over sequences |
| Invoke | `INVOKE(method, args...)` | Call external plugin method |
| Let | `LET` | Immutable variable binding |
| If / Else | `IF / ELSE` | Conditional branch |
| Use Plugin | `USE PLUGIN::<name>` | Declare plugin namespace scope |

---

## 🔌 FFI — Type Contracts

| CEL Type | C Equivalent | WASM Type | Size |
|---|---|---|---|
| `Number` | `double` | `f64` | 8 bytes |
| `Bool` | `uint8_t` | `i32` | 1 byte |
| `Text` | `const char*` | `i32` (ptr) | pointer |
| `Bytes` | `uint8_t*` + `size_t` | `i32` + `i32` | ptr + len |
| `Vector` | `float*` + `size_t` | `i32` + `i32` | ptr + len |
| `Null` | `void` | — | 0 |

---

## ⚙️ Configuration — `system_control.json` Fields

| Field | Type | Description |
|---|---|---|
| `node_id` | String | Unique workspace identifier for local system discovery |
| `active_model` | String | Model loaded in current session |
| `user_identity.purpose` | String | `RESEARCH` (precision) or `PRODUCTION` (speed) |
| `hardware_governance.vram_limit_gb` | Float | Hard ceiling for VRAM allocations |
| `hardware_governance.cpu_thread_limit` | Integer | Max BLAS thread concurrency |
| `hardware_governance.allow_speculative_decoding` | Boolean | Permits draft model execution |
| `hardware_governance.fallback_to_cpu` | Boolean | Graceful CPU fallback if GPU hits OOM |
| `network.api_host` | String | Axum HTTP bind address |
| `network.api_port` | Integer | Engine network port |

---

## 🚀 Configuration — `system_booster.json` Fields

| Field | Type | Valid Values | Description |
|---|---|---|---|
| `mode_run` | String | `edge` / `balance` / `max_boost` / `ultra_max_boost` | CPU thread scheduler priority |
| `n_gpu_layers` | Integer | `-1` to `128` | Layers offloaded to GPU (`-1` = full) |
| `kv_quant` | String | `auto` / `kv16` / `kv8` / `kv4` | KV cache compression level |
| `context_shift` | String | `auto` / `minimal` / `aggressive` | Rolling context window compression |
| `flash_attention` | String | `on` / `off` / `auto` | SRAM Flash Attention tiling |
| `speculative_decoding` | String | `on` / `off` / `auto` | Draft-model speculative generation |
| `turbo_quant` | String | `on` / `off` / `auto` | Givens rotation quantization correction |
| `think_mode` | String | `on` / `off` / `auto` | Chain-of-Thought internal reasoning |
| `enforce_json` | Boolean | `true` / `false` | Force strictly valid JSON output |
| `force_memory_lock` | String | `on` / `off` | `mlock` / `VirtualLock` to prevent swap |

---

## 🖥️ Hardware Layer Terms

| Term | Definition |
|---|---|
| **AVX512 / AVX2 / NEON** | CPU SIMD instruction set extensions for vectorized tensor math |
| **CUDA** | NVIDIA GPU compute API — highest performance GPU backend |
| **Metal** | Apple GPU compute API — used on macOS / Apple Silicon |
| **Vulkan** | Cross-platform GPU compute API — Linux / Windows / Android |
| **BLAS** | Basic Linear Algebra Subprograms — optimized matrix multiplication routines |
| **mlock** | Linux syscall to pin memory pages, preventing OS from swapping them |
| **VirtualLock** | Windows equivalent of `mlock` |
| **PCIe Shared Memory** | Fallback GPU memory path — 10-100x slower than native VRAM |
| **Speculative Decoding** | Draft model generates candidate tokens, primary model verifies — 2-4x throughput gain |
| **Flash Attention** | SRAM tiling algorithm for attention computation — reduces VRAM from O(n²) to O(n) |

---

## 🏷️ Reserved Prefixes & URI Schemes

| Prefix | Usage |
|---|---|
| `$` | Dynamic variable reference (`$user_id`) |
| `@` | Decorator / annotation (`@plugin`) |
| `::` | Namespace separator (`PLUGIN::method`) |
| `cel://` | CEL resource URI scheme (`cel://local/executor`) |
| `~/.cluaiz/` | Default workspace root directory |

---

## 📋 Runtime Constants

| Constant | Default | Description |
|---|---|---|
| `MAX_PIPELINE_DEPTH` | `64` | Max chained `->` operators per CEL expression |
| `MAX_VECTOR_DIM` | `4096` | Max embedding vector dimensions |
| `DEFAULT_TIME_WINDOW` | `300s` | Default temporal filter range |
| `FFI_STACK_SIZE` | `8 MiB` | Per-call FFI stack allocation |
| `WAL_BUFFER_SIZE` | `4 MiB` | Write-Ahead Log in-memory buffer before flush |
| `DEFAULT_API_PORT` | `3000` | Default Axum HTTP engine port |
| `DEFAULT_KV_CACHE_QUANT` | `auto` | Default KV cache quantization level |
