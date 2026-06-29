# Cluaiz Execution Language (CEL) Master Reference Manual

Welcome to the Master Specifications Manual for the **Cluaiz Execution Language (CEL)**. This document consolidates the syntax rules, dynamic value layouts, lexical token references, authoring guidelines, use cases, and SDK integrations.


CEL is a strict, hardware-accelerated pipeline language designed to bridge your host application (C/C++, Python, Go, Node.js) with high-performance Rust execution engines (WASM sandboxes, SIMD pipelines, and Native FFI modules) at zero latency.

---

## 🏛️ 1. Syntax & Grammar Rules

All CEL statements fall into one of four primary grammar blocks: expressions, assignments, conditional branches, or iterations.

### A. Core Pipelines & Operators
A CEL statement is executed as a pipeline chain using the `->` operator. Each segment of the pipeline corresponds to a specific AST operation.
```text
use plugin::database -> find User(id: 42) -> select(username, email)
```

### B. Variable Assignments (`let`)
Saves state intermediate results inside the engine memory to prevent expensive round-trips to the host agent.
```text
let $result = use plugin::scrapper -> extract(url: "https://cluaiz.com");
```

### C. Control Flow (`if / else`)
Natively routes computation branches inside the engine at CPU speed.
```text
if ($user.is_active) {
    use plugin::database -> update User(id: $user.id, last_seen: "now");
} else {
    use plugin::alerts -> notify(user: $user.username, type: "dormant");
}
```

### D. Iterators (`foreach`)
Loops over lists and arrays natively inside the execution thread.
```text
foreach ($id in $user_ids) {
    use plugin::database -> delete User(id: $id);
}
```

---

## 💎 2. AST Value Types

The dynamic value layout parser strictly dictates the following data types:

| Type | Syntax Example | Serialized Output |
|:---|:---|:---|
| **`Text`** | `"Hello World"` | UTF-8 String slice |
| **`Number`** | `42.5` | `f64` Float precision |
| **`Bool`** | `true` | `bool` Boolean |
| **`Vector`** | `[0.1, -0.2, 0.9]` | `Vec<f32>` High-dimensional embeddings |
| **`Variable`** | `$user_profile` | Evaluated key lookup |
| **`Null`** | `null` | Zero allocation unit |

---

## ⚙️ 3. Syntax & Operator Reference (Lexer Tokens)

Below is the exhaustive specification for each instruction option parsed by the lexer.

* ### `use plugin::<name>`
  * **Syntax:** `use plugin::<name>`
  * **Description:** Resolves paths and dynamic dependencies. For Native Dynlibs, it dynamically links using `libloading` with canonicalized paths. For WASM, it mounts the binary directly into the global execution cache.
  * **Reference:** [Dynamic Linking Tutorial](../cel/tutorials/dynamic_linking.md)

* ### `invoke(<method>, args...)`
  * **Syntax:** `use plugin::auth -> invoke(verify, token: "xyz")`
  * **Description:** Resolves FFI parameters and triggers the method execution. For WASM plugins, it enforces strict CPU instruction fuel limits and RAM allocations.
  * **Reference:** [Invoke Methods Tutorial](../cel/tutorials/invoke.md)

* ### `filter`
  * **Syntax:** `-> filter age > 18`
  * **Description:** Performs native comparisons (using `>`, `<`, `==`) on the CPU/SIMD layers to prune datasets before memory allocation.
  * **Reference:** [Filter Operator Tutorial](../cel/tutorials/filter.md)

* ### `process(<text>)`
  * **Syntax:** `process("Raw Input text")`
  * **Description:** Bypasses heavy VM compiler initialization for fast-path string/text manipulations.
  * **Reference:** [Process Operator Tutorial](../cel/tutorials/process.md)

* ### `select`
  * **Syntax:** `-> select(username, email)`
  * **Description:** Projects and strips unused fields from serialized envelopes to prevent VRAM memory bloat.
  * **Reference:** [Select Operator Tutorial](../cel/tutorials/select.md)

* ### `similar`
  * **Syntax:** `-> similar_to(vector: [...], metric: "cosine")`
  * **Description:** Dispatches cosine/Euclidean vector similarity scans directly to the CPU SIMD registers or GPU vector cores.
  * **Reference:** [Hardware Vector Search Tutorial](../cel/tutorials/hardware_vector_search.md)

* ### `time_window`
  * **Syntax:** `-> time_window(size: "1h")`
  * **Description:** Sets limits on context windows to prevent KV Cache depletion.
  * **Reference:** [Time Window Tutorial](../cel/tutorials/time_window.md)

* ### `find`
  * **Syntax:** `find User(id: 42)`
  * **Description:** Core query command. Fetches records from the primary database backend database.
  * **Reference:** [Getting Started Tutorial](../cel/tutorials/getting_started.md)

* ### `foreach`
  * **Syntax:** `foreach ($id in $ids)`
  * **Description:** Turing-complete iteration loops over arrays using WASM linear memory buffers.
  * **Reference:** [Iterating Data Tutorial](../cel/tutorials/iterating_data.md)

* ### `if / else`
  * **Syntax:** `if ($cond) { ... } else { ... }`
  * **Description:** Conditional branching at the AST level to prune unexecuted paths.
  * **Reference:** [Control Flow Tutorial](../cel/tutorials/control_flow.md)

* ### `let`
  * **Syntax:** `let $var = ...`
  * **Description:** Allocates and binds a variable inside the execution frame hashmap.
  * **Reference:** [Variable Assignments Tutorial](../cel/tutorials/variable_assignments.md)

* ### `->` (Pipe)
  * **Syntax:** `op1 -> op2`
  * **Description:** The memory barrier operator passing raw binary `ExtensionPayload`.
  * **Reference:** [Getting Started Tutorial](../cel/tutorials/getting_started.md)

* ### `cel://local/executor`
  * **Syntax:** `cel://local/executor`
  * **Description:** Internal URI scheme determining target executor for local scripts.
  * **Reference:** [Executor Protocol Tutorial](../cel/tutorials/executor_protocol.md)

* ### `?` (Parameter)
  * **Syntax:** `find User(id: ?)`
  * **Description:** Halts text evaluation to bind massive binary payloads without parsing overhead.
  * **Reference:** [Parameterized Queries Tutorial](../cel/tutorials/parameterized_queries.md)

---

## 🔒 4. Hardcore Engine Control Directives

These commands bypass typical plugins to inject state commands directly into the core runtime scheduler.
* **KV Cache Control (`engine -> kv_cache -> clear($user_id)`)**: Reclaims GPU memory allocated to the selected user session's attention layers. Reference: [Engine Directives Guide](../cel/tutorials/engine_directives.md).
* **Middle-Layer Injection (`engine -> mid_layer -> inject($data)`)**: Bypasses autoregressive generation loops to write context directly into attention matrixes. Reference: [Engine Directives Guide](../cel/tutorials/engine_directives.md).
* **Inference Scheduling (`engine -> inference -> pause()`)**: Directs tokio execution threads to yield compute priority. Reference: [Engine Directives Guide](../cel/tutorials/engine_directives.md).
* **OS Process Control (`engine -> os -> process("ps")`)**: Spawns isolated subprocesses to monitor hardware health. Reference: [Engine Directives Guide](../cel/tutorials/engine_directives.md).

---

## 🛡️ 5. Sandbox Security & Authoring Guidelines

Learn the security models and authoring boundaries for executing CEL logic.
* **Execution Sandboxes**: Detailed comparison between WebAssembly (Strict 64KB Isolation ring), Rhai scripts (Unbounded loops), and Pure CEL (AST bounded checks). Reference: [WASM vs Rhai vs Pure Sandboxes](../cel/authoring/wasm_vs_rhai_vs_pure.md).
* **Anti-Patterns**: Why you should never embed CEL commands directly inside JSON, YAML, or Markdown configurations. Reference: [Embedding Rules Protocol](../cel/authoring/embedding_rules.md).
* **Pure CEL Files**: Why keeping CEL script logic in standalone `.cel` files is critical for parser performance. Reference: [Pure CEL Files Manual](../cel/authoring/pure_cel_files.md).

---

## 🌐 6. Native SDKs Integration

Learn how to integrate the CEL engine into your backend using zero-overhead C-ABI pointers (`ExtensionPayload`) instead of slow HTTP protocols.
* **SDK Protocol Overview**: Standard architecture manual. Reference: [SDK Master Overview](../cel/sdk/sdk.md).
* **C FFI Integration**: Zero-copy allocations using pure structs. Reference: [C FFI SDK](../cel/sdk/c-ffi.md).
* **C++ FFI Integration**: Bridging pointers directly into C++ engines. Reference: [C++ FFI SDK](../cel/sdk/cpp-ffi.md).
* **Go Integration**: Bridge Go applications using `cgo` and memory pointers. Reference: [Go FFI SDK](../cel/sdk/go-ffi.md).
* **Node.js Integration**: Bind Javascript engines via `node-ffi-napi`. Reference: [NodeJS FFI SDK](../cel/sdk/nodejs-ffi.md).
* **Python Integration**: Bridge Python scripts using `cffi`. Reference: [Python CFFI SDK](../cel/sdk/python-cffi.md).
* **Rust Native Integration**: Low-level native crate bindings. Reference: [Rust Native SDK](../cel/sdk/rust-native.md).
* **Pure CEL Compiler**: Zero-latency Rust AST compiler configurations. Reference: [Pure CEL SDK](../cel/sdk/pure-cel.md).
* **Custom WASM Plugin**: Compiling Rust/AssemblyScript targets to WASM. Reference: [Custom WASM Plugin](../cel/sdk/custom-wasm-plugin.md).
* **Manifest Architecture**: Plugin/Extension lifecycle manifests. Reference: [Manifest Architecture](../cel/sdk/manifest-architecture.md).

---

## 🚀 7. Real-World Use Cases

See how multiple AI plugins chain tasks inside VRAM without passing intermediate buffers back to host languages.
* **RAG Pipeline**: Vector Database querying and LLM context synthesis in a single pipeline. Reference: [RAG Pipeline Use Case](../cel/usecases/rag_pipeline.md).
* **Vision Agent**: Orchestrating vision embeddings, OCR extraction, and summaries. Reference: [Vision Agent Use Case](../cel/usecases/vision_agent.md).
* **Log Processor**: Fast log stream filtering and parsing. Reference: [Log Processor Use Case](../cel/usecases/log_processor.md).
