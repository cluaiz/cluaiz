# 📖 cluaiz Execution Language (CEL)

Welcome to the **CEL Architecture and Reference Index**. This is the central hub for understanding, authoring, and integrating the cluaiz Execution Language (CEL) into your backend systems.

CEL is not a generic scripting language; it is a strict, hardware-accelerated pipeline language designed to bridge your host application (Python/Go/C++) with high-performance Rust execution engines (WASM, SIMD, Native FFI).

---

## 1. Core Architecture & SDKs (`/docs/cel/sdk/`)

Learn how to integrate the cluaiz Engine into your backend using zero-overhead C-ABI pointers (`ExtensionPayload`). HTTP integration is strictly prohibited.

| Topic | Description | Deep Dive |
|---|---|---|
| **SDK Protocol Overview** | Why HTTP is an anti-pattern and how Native FFI works. | [Read Concept](../cel/sdk/sdk.md) |
| **Direct `.cel` Execution** | How the Engine compiles ASTs and implements Zero-Latency Cold Boots. | [Read Pure CEL](../cel/sdk/pure-cel.md) |
| **Go (cgo) SDK** | Integrating CEL via `cgo`, managing `C.malloc` memory. | [Read Go FFI](../cel/sdk/go-ffi.md) |
| **C/C++ SDK** | The fastest native integration using pure C structs and pointers. | [Read C FFI](../cel/sdk/c-ffi.md) / [C++ FFI](../cel/sdk/cpp-ffi.md) |
| **Python SDK** | Bridging Python to Rust using `cffi`. | [Read Python FFI](../cel/sdk/python-cffi.md) |
| **Node.js SDK** | Calling Rust via `node-ffi-napi`. | [Read Node FFI](../cel/sdk/nodejs-ffi.md) |

---

## 2. Authoring Guidelines (`/docs/cel/authoring/`)

Understand the constraints, security models, and best practices for writing CEL logic.

| Topic | Description | Deep Dive |
|---|---|---|
| **Execution Sandboxes** | Differences between WASM (Strict 64KB Isolation), Rhai (Unbounded), and Pure CEL (AST bounded). | [Read Matrix](../cel/authoring/wasm_vs_rhai_vs_pure.md) |
| **Embedding Anti-Patterns** | Why you should never embed CEL strings inside JSON, YAML, or Markdown files. | [Read Rules](../cel/authoring/embedding_rules.md) |
| **Native `.cel` Files** | The benefits of keeping CEL logic in isolated `.cel` files (IDE tooling, parallel parsing). | [Read Benefits](../cel/authoring/pure_cel_files.md) |

---

## 3. Real-World Use Cases (`/docs/cel/usecases/`)

See how CEL is used to chain multiple AI plugins efficiently without passing memory back and forth to the host language.

| Use Case | Description | Deep Dive |
|---|---|---|
| **RAG Pipeline** | Vector DB + LLM synthesis in a single 4-line CEL pipeline. | [Read RAG](../cel/usecases/rag_pipeline.md) |
| **Vision Agent** | Connecting OCR plugins with summarization logic. | [Read Vision](../cel/usecases/vision_agent.md) |
| **Log Processor** | High-speed data filtering and transformation. | [Read Logs](../cel/usecases/log_processor.md) |

---

## 4. Syntax & Keyword Reference (`/docs/cel/tutorials/`)

This table covers the core lexical tokens parsed by `lexer.rs`.

| Keyword / Operator | Technical Execution Context | Deep Dive |
|---|---|---|
| `cel://local/executor` | Understanding execution routing for multi-language scripts. | [Read Tutorial](../cel/tutorials/executor_protocol.md) |
| `?` (Parameter) | Halts text evaluation to bind massive binary payloads without parsing overhead. | [Read Tutorial](../cel/tutorials/parameterized_queries.md) |
| `engine` | Hardcore host-level directives bypassing plugins (KV Cache clearing). | [Read Tutorial](../cel/tutorials/engine_directives.md) |
| `filter` | Drops memory payloads based on native CPU/SIMD comparisons (`>`,`<`,`==`). | [Read Tutorial](../cel/tutorials/filter.md) |
| `find` | Core query command. Fetches records from the primary database backend. | [Read Tutorial](../cel/tutorials/getting_started.md) |
| `foreach` | Turing-complete iteration over arrays using a single WASM linear memory block. | [Read Tutorial](../cel/tutorials/iterating_data.md) |
| `if / else` | Conditional branching at the AST level. Prunes branches to prevent allocations. | [Read Tutorial](../cel/tutorials/control_flow.md) |
| `invoke` | Calls a specific method inside a loaded plugin, mapping arguments to C-ABI. | [Read Tutorial](../cel/tutorials/invoke.md) |
| `let` | Allocates a variable inside the execution frame's hashmap to cache data. | [Read Tutorial](../cel/tutorials/variable_assignments.md) |
| `->` (Pipe) | The memory barrier operator passing raw binary `ExtensionPayload`. | [Read Tutorial](../cel/tutorials/getting_started.md) | 
| `process` | Fast-path execution for raw string/text manipulations. | [Read Tutorial](../cel/tutorials/process.md) |
| `select` | Memory projection. Strips unused fields to prevent VRAM bloat. | [Read Tutorial](../cel/tutorials/select.md) |
| `similar_to` | Dispatches hardware-accelerated vector similarity scans to SIMD cores. | [Read Tutorial](../cel/tutorials/hardware_vector_search.md) |
| `time_window` | Truncates context limits for loops to stay within KV Cache windows. | [Read Tutorial](../cel/tutorials/time_window.md) |
| `use` | Loading capabilities | [Dynamic Linking (Plugins, Extensions, MCP)](../cel/tutorials/dynamic_linking.md) |
