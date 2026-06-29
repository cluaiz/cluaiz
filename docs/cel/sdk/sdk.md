---
title: cluaiz Native SDK (CEL Bindings)
description: The master guide to using the CEL Native SDK across languages, including WASM and Rhai execution environments.
---

# The cluaiz Native SDK

The **CEL SDK** provides bindings for Node.js, Python, Rust, and Go to execute cluaiz Execution Language (CEL) scripts. 

Unlike traditional SDKs that connect to an external server via HTTP, the cluaiz SDK operates via **Native FFI (Foreign Function Interface)**. You embed the cluaiz Engine (`cluaiz.dll` or `libcluaiz.so`) directly into your host language's memory space.

## 🛑 Anti-Patterns (How NOT to Use CEL)

### 1. The HTTP Loophole
You should **never** wrap the cluaiz Engine behind an HTTP server (e.g., FastAPI, Express) and send raw CEL strings over the network for execution.
- **The Security Loophole:** Exposing a network endpoint that accepts and executes dynamic CEL code allows unbounded execution. While the internal parser has a `MAX_PARSE_DEPTH = 32`, an attacker could still trigger Denial of Service via massive `foreach` loops if the HTTP layer isn't heavily guarded. Native FFI SDKs keep the execution completely contained and sandboxed within your secure backend process.
- **The Performance Bottleneck:** The cluaiz SDK uses the C-ABI `ExtensionPayload` pointer for memory transfers. You cannot send a raw RAM pointer over HTTP. Serializing a 10MB tensor payload to JSON over HTTP takes ~500ms. Transferring it via the Native SDK (FFI Bincode pointers) takes **0.05ms**.

### 2. Embedding CEL inside JSON, YAML, or Markdown
You should **never** embed CEL code inside JSON payloads, YAML configurations, or Markdown (`.md`) code blocks with the expectation that the cluaiz engine will natively extract and run it.
- **Codebase Reality:** The `lexer.rs` parses pure CEL strings. While `metadata_parser.rs` parses `SKILL.md` files, it **only** extracts the YAML frontmatter for metadata; it does **not** extract and execute CEL code blocks from the markdown body.
- **Why it's bad:** Passing `{"script": "let $x = 1"}` adds JSON serialization overhead and strips your IDE of CEL syntax highlighting and compiler safety.
- **The Optimal Way:** Pass **Pure CEL strings** directly through the Native SDK, or load them from `.cel` files directly into memory.

---

## Execution Architectures (WASM vs Rhai vs Native)

When you send a CEL string through the SDK, the Rust Engine parses the AST and routes the logic to one of three execution environments.

### 1. WASM Sandboxing (Strict Isolation)
Defined in `wasm_host.rs`, cluaiz uses `wasmtime` to execute CEL logic inside WebAssembly modules.
- **64KB Memory Isolation:** Every WASM plugin operates in a strict, isolated linear memory space. It cannot read the host environment's RAM.
- **[ARCHER-GUARD]:** System calls are actively intercepted. If a CEL script attempts to break out of the sandbox, the `cluaiz_host_call` ABI hook catches it.
- **Usage:** Used for untrusted plugins, LLM-generated code, or third-party logic.

### 2. Legacy Rhai Engine (Trusted Execution)
Defined in `legacy_rhai.rs`, the engine can drop down to execute raw Rhai scripts.
- **No Fuel Limits:** Unlike WASM, the Legacy Rhai executor lacks strict `wasmtime`-level fuel limits for DoS prevention.
- **Usage:** Used ONLY for highly trusted, internal configuration logic where raw performance and scripting flexibility are prioritized over hard security boundaries.

### 3. Native SDK (FFI)
The logic executes natively in Rust, but passes data to your host language (Node.js/Python) via `ExtensionPayload` pointers.

---


