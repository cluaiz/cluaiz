---
title: The cel://local/executor Protocol
description: Understanding the execution routing for multi-language scripts (CEL, Rhai, WASM, V8).
---

# The Executor Protocol

When interfacing with the cluaiz Inference Engine via its API or internal routing, you'll often see the scheme:
`cel://local/executor`

This is the primary endpoint for submitting scripts or payloads to the cluaiz execution loop.

## What is `cel://local/executor`?

Unlike a standard HTTP REST endpoint (`http://api.local/execute`), the `cel://` scheme denotes a direct **in-memory invocation**. 

When a payload is sent to this endpoint, it completely bypasses the network stack and TCP overhead. The engine grabs the raw memory pointer of your request and routes it to the **Unified Executor Enum** (`Cluaizxecutor`).

## Multi-Engine Support

As seen in the cluaiz UI dropdown, the Engine doesn't just run CEL. It acts as an orchestrator for multiple execution tiers. You can submit scripts in different languages, and the engine routes them to the appropriate sandboxed executor.

### The Hardware Reality (Execution Tiers)
In `cluaiz/inference-cel/src/execution/mod.rs`, the engine defines the execution router:

```rust
pub enum Cluaizxecutor {
    Wasm(wasm_sandbox::WasmExecutor),
    Native(native_sandbox::NativeExecutor),
    Rhai(legacy_rhai::LegacyRhaiExecutor),
}
```

When you hit `cel://local/executor`, you must specify the target language in your payload (or UI). The engine routes it as follows:

#### 1. CEL (cluaiz Engine Language)
- **Target:** Native AST Evaluator
- **Execution:** Rust Native (0ms latency)
- **Use Case:** Orchestrating pipelines, calling WASM plugins, and filtering memory streams.

#### 2. WASM (Rust/C++)
- **Target:** `WasmExecutor` (`wasmtime`)
- **Execution:** Sandboxed JIT (Strict isolation, fuel constraints)
- **Use Case:** Heavy compute, neural network pre-processing, community-built logic.

#### 3. Rhai Script (Legacy)
- **Target:** `LegacyRhaiExecutor`
- **Execution:** Embedded AST Interpreter (Engine main thread)
- **Use Case:** Rapid prototyping, simple logic where WASM compilation is overkill. Kept strictly for backward compatibility with early plugins.

#### 4. JavaScript (V8)
- **Target:** `V8 Isolate` (via Native/Wasm bridge depending on build configuration)
- **Execution:** JIT Compilation
- **Use Case:** Front-end developers injecting dynamic logic without needing to compile Rust to WASM.

## How it works (Example)

When you send a Rhai Script to `cel://local/executor`, the engine doesn't invoke the CEL lexer. Instead, it directly mounts the script inside `LegacyRhaiExecutor`. The engine injects metadata into the execution scope:

```rust
scope.push("plugin_name", "legacy_script");
scope.push("is_fast_path", false);
```

This ensures that regardless of the language you choose in the UI, it conforms to the Engine's zero-copy memory architecture and security policies.
