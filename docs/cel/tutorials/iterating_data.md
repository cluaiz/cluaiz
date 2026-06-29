---
title: Iterating Data (foreach)
description: Iterating over collections inside CEL using zero-copy linear memory loops.
---

# Iterating Data (`foreach`)

Processing lists of data without network overhead is a critical feature of CEL. Instead of returning an array to your API and running a `for` loop, you run the loop directly inside the Engine.

## Syntax
```cel
foreach ($item in $list) {
    <Pipeline>
}
```

## The Hardware Reality (WASM Linear Memory)

Loops can be computationally expensive if they spin up new contexts or re-parse variables on every iteration. The cluaiz Engine optimizes `foreach` loops specifically for `CelValue::Vector` and Arrays.

```rust
// Internally in the Rust Engine (inference-cel/src/parser/ast.rs)
pub enum CelStatement {
    Foreach {
        item_var: String,
        list_var: String,
        block: Box<CelAst>,
    }
}
```

**Linear Memory Re-use:**
When a `foreach` loop triggers a `use plugin::...` block inside it, the Engine does **not** re-initialize the WASM instance (`wasmtime::Store`) for every item. Instead, the Engine holds the WASM instance open and simply overwrites the linear memory buffer with the next item's `ExtensionPayload` pointer. 
This results in microsecond-level iteration speeds.

```mermaid
flowchart TD
    A["foreach ($item in $list)"] --> B{"Lexer parser (lexer.rs)"}
    
    B -->|Generates| C["CelStatement::Foreach"]
    C -->|List Pointer| D{"Engine WASM Executor"}
    
    D -->|Initialize Once| E["wasmtime::Store"]
    
    E --> F["Write Item 1 to Linear Memory"]
    F -->|Execute| G["WASM Plugin"]
    
    G --> H["Overwrite Linear Memory with Item 2"]
    H -->|Execute (Zero Sandbox Latency)| G
```

## 🚨 Security: Stack Overflow Protection (C-2)

Because `foreach` loops can be nested, the Lexer inherently protects the Rust Host from malicious scripts attempting to crash the engine via infinite recursion stack overflows. 

According to `lexer.rs`:
```rust
const MAX_PARSE_DEPTH: usize = 32;
```
If the nesting depth exceeds `32` levels, the Engine instantly throws a parsing error (`"CEL nesting depth exceeded"`) and refuses to execute the AST.

## Deep Dive Example: Batch Processing

Imagine you have a payload containing a list of transaction IDs, and you want to fetch and process all of them via an external microservice.

```cel
let $payload = use plugin::api_gateway -> invoke(get_request_body)

// $payload.transactions is an array: ["txn_1", "txn_2"]
foreach ($txn_id in $payload.transactions) {
    
    // The memory stream is passed sequentially
    use plugin::stripe -> invoke(get_txn, id: $txn_id) 
        -> filter amount > 100 
        -> use plugin::fraud_detector 
        -> invoke(scan)
}
```

## 🚨 Common Mistake: CPU Thrashing
**Do NOT** declare `use plugin::` inside the loop if it's the exact same plugin being called globally. While WASM linear memory is reused, resolving the plugin name repeatedly still takes cycles.

**❌ Bad (Thrashing):**
```cel
foreach ($doc in $docs) {
    let $processed = $doc -> use plugin::text_parser -> invoke(clean)
}
```

**✅ Good (Pre-loading):**
Currently, `use plugin::` dynamically resolves in the AST. In production scale, always group your data into a pipeline *before* passing it to a plugin if the plugin supports batching, or rely on the Engine's `WASM_CACHE` to mitigate the load time.
