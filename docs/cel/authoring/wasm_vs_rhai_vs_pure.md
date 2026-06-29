---
title: CEL Authoring Environments
description: Understanding the differences between authoring for WASM, Rhai, and Pure CEL.
---

# Authoring CEL: WASM vs Rhai vs Pure

When writing logic for the cluaiz Engine, you are not bound to a single execution paradigm. The engine supports three distinct environments, each with different security models, capabilities, and authoring rules.

---

## 1. Pure CEL (The Native AST)

Pure CEL (cluaiz Execution Language) is the core syntax parsed by `lexer.rs`. 

**Authoring Rules:**
- **File Extension:** `.cel` (or passed as a string via the SDK).
- **Security:** Highly secure. Execution is strictly bound by the engine's parsing depth limits (`MAX_PARSE_DEPTH = 32`).
- **Features:** It is a declarative pipeline language. You chain operations using the arrow operator `->`.
- **When to use:** When you need the absolute fastest, zero-overhead pipeline execution.

**Example (Pure CEL):**
```cel
let $data = use plugin::database -> invoke(fetch_users)
$data -> filter(age > 18) -> select(email)
```

---

## 2. Rhai Scripting (Legacy / Trusted)

Rhai is an embedded scripting language natively executed by the cluaiz engine (via `legacy_rhai.rs`).

**Authoring Rules:**
- **File Extension:** `.rhai`
- **Security:** **WARNING**. Rhai execution in cluaiz lacks fuel-based DoS prevention. There is no hard timeout or fuel limit enforced at the WASM boundary level.
- **Features:** Imperative scripting. You can write loops, mutate variables, and write complex conditionals that are difficult to express in declarative CEL.
- **When to use:** Only for highly trusted, internal configurations where you need imperative logic and can guarantee the script won't infinite-loop.

**Example (Rhai):**
```rust
let users = engine.fetch_users();
let mut valid_emails = [];

for user in users {
    if user.age > 18 {
        valid_emails.push(user.email);
    }
}

return valid_emails;
```

---

## 3. WASM Sandboxing (Strict Isolation)

You can write logic in Rust, C, or Go, compile it to WebAssembly (`.wasm`), and the engine will execute it using `wasmtime` (`wasm_host.rs`).

**Authoring Rules:**
- **File Extension:** `.wasm`
- **Security:** **Highest Security**. The WASM plugin is given a strict 64KB linear memory sandbox. If it tries to allocate too much, or burn too much CPU time, the Engine kills it instantly via fuel limits. System calls are actively intercepted by `[ARCHER-GUARD]`.
- **Features:** Full Turing-complete logic using your language of choice. However, string/pointer passing requires linear memory management.
- **When to use:** When executing third-party logic, LLM-generated code, or any untrusted logic that must not compromise the host system.

**Example (Rust compiled to WASM):**
```rust
#[no_mangle]
pub extern "C" fn run_logic() {
    // This executes securely within the 64KB Wasmtime Sandbox
    let message = "Hello from WASM Sandbox!";
    cluaiz_host_call(message.as_ptr(), message.len());
}
```

---

## Summary Matrix

| Environment | Authoring Lang | Execution Paradigm | Security / Isolation | Best For |
|---|---|---|---|---|
| **Pure CEL** | `.cel` syntax | Declarative Pipeline | High (AST bounds) | Data pipelines, RAG |
| **Rhai** | `.rhai` syntax | Imperative Scripting | Low (No fuel limits) | Trusted internal config |
| **WASM** | Rust / C / Go | Fully Compiled Binary | Extreme (64KB Sandbox) | Untrusted / 3rd Party plugins |
