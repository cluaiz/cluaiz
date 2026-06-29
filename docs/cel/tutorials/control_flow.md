---
title: CEL Control Flow (if / else)
description: Natively evaluating boolean logic and AST branch pruning inside the cluaiz Engine.
---

# Control Flow (`if / else`)

CEL allows you to write conditional decision trees that execute entirely inside the Rust engine, avoiding the latency of sending data back to a Node.js or Python backend just to make an `if` statement check.

## Syntax
```cel
if (<condition>) {
    <Pipeline A>
} else {
    <Pipeline B>
}
```

## The Hardware Reality (AST Branch Pruning)

When the CEL parser hits an `if` statement, it constructs an `IfElse` node in the AST containing two separate `CelAst` block pointers (`if_block` and `else_block`).

```rust
// Internally in the Rust Engine (inference-cel/src/parser/ast.rs)
pub enum CelStatement {
    IfElse {
        condition: String,
        if_block: Box<CelAst>,
        else_block: Option<Box<CelAst>>,
    }
}
```

**Zero-Allocation Branching:**
The Engine evaluates the `<condition>`. It natively resolves to a Rust `bool`. 
If `true`, the engine executes `if_block`. The `else_block` is **completely ignored and pruned**. No memory is allocated for the unexecuted branch. This is significantly faster than WASM-based branching because it happens natively in the Rust execution thread before the WASM sandbox is even spun up.

## 🚨 Security: Stack Overflow Protection (C-2)

Because CEL supports nested `if` and `foreach` statements, a malicious or poorly written script could attempt to infinitely nest blocks to cause a Stack Overflow on the Rust Host. 

To mathematically prevent this, the CEL lexer enforces a hardcoded depth constraint (`lexer.rs`):

```rust
const MAX_PARSE_DEPTH: usize = 32;
```

If the nesting depth exceeds `32` levels, the Engine instantly throws a parsing error (`"CEL nesting depth exceeded"`) and refuses to execute the AST, saving the host hardware from crashing.

## Deep Dive Example: Auth Verification

A common use case is validating a user's role before executing an expensive AI operation.

```cel
let $user = use plugin::auth -> invoke(get_session)

if ($user.role == "admin") {
    // 0ms execution path. Triggers the heavy AI plugin.
    $user -> use plugin::admin_dashboard -> invoke(generate_report)
} else {
    // Falls back to a fast string process
    process("Access Denied")
}
```

### Supported Condition Operators
The `if` condition supports the same native byte-evaluation logic as the `filter` command:
- `==` / `!=` (Equality)
- `>` / `<` / `>=` / `<=` (Numeric Comparison)
- `contains` (Array/String inclusion)
