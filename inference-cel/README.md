# cluaiz Inference CEL (cluaiz Expression Language) 🧠⚡

**The cluaiz Brain Router, Zero-Overhead FFI Transducer, & Turing-Complete Orchestration Engine for Autonomous AI**

`inference-cel` is the cornerstone architectural component of the cluaiz ecosystem. It solves the **"Monolith Trap"**—the flaw in other platforms that bloat their core binaries by hardcoding database drivers, web scrapers, and external APIs.

Instead of hardcoding logic, `inference-cel` acts as a **"Dumb, Ultra-Fast Router"**. It has zero knowledge of what a Database, Web Search, or API is. It only knows how to parse a universal orchestration language (CEL) into a Turing-Complete Abstract Syntax Tree (AST), and execute it across strict memory-safe boundaries at bare-metal speeds (0.05ms latency).

---

## 🌟 1. The Core Philosophy: "One DNA for the Entire Ecosystem"

Think of it like the English language:
- **CEL** is the grammatical structure (`->` chaining, variables, if/else, loops).
- **CDQL (Cluaiz Database Query Language)** is just the "Database Vocabulary" spoken using CEL grammar.
- **Web-Search** is another vocabulary spoken using CEL grammar.

Because the grammar is 100% identical, the cluaiz Engine only needs **ONE Universal Parser**. The LLM agent generates a CEL script, the engine parses it, plans it via `planner.rs`, and routes it natively via C-FFI or WebAssembly (WASM).

---

## 🛠️ 2. The Turing-Complete AST & Syntax (`lexer.rs` & `ast.rs`)

CEL is not just a linear pipeline; it is a full programming paradigm designed for AI Autonomous Agents. All logic is parsed using a custom **Recursive Descent Parser** in `lexer.rs` and mapped to a hierarchical tree in `ast.rs`.

### A. Variables & State Retention (`let`)
Agents can store context natively inside the Engine without round-tripping to the LLM, saving massive overhead.
```cel
let $active_users = use plugin::database -> find User(status: "active");
use plugin::email -> send_bulk(users: $active_users);
```

### B. Control Flow (`if / else`)
Decision trees execute at zero-latency Native Rust speeds.
```cel
let $check = use plugin::auth -> verify(token: "xyz");
if ($check.is_valid) {
    use plugin::database -> find User(id: $check.uid);
} else {
    process("Unauthorized Error");
}
```

### C. Iterators (`foreach`)
Array and list processing handled directly inside the core engine.
```cel
let $users = use plugin::database -> find User -> limit 5;
foreach ($user in $users) {
    use plugin::email -> send(to: $user.email);
}
```

### D. CDQL Native Commands & Structural Filters
Full structural support for complex database commands, operators (`>=`, `<=`, `!=`), and boolean filters. The `Filter` struct retains field and operator types.
```cel
use plugin::database -> find Neuron -> filter age >= 18 -> sort desc
```

### E. Projections & Memory Optimization
```cel
use plugin::database -> find User -> select(id, name, email)
```

### F. [NEW] Hardcore Native Engine Directives
CEL acts as a direct control protocol for the `neural_foundry`. The LLM or external APIs can natively pause inference, flush Key-Value caches, or inject semantic data directly into the active layer without routing through any plugin or WASM boundary.
```cel
engine -> kv_cache -> clear($user_session)
engine -> mid_layer -> inject($retrieved_data)
engine -> inference -> pause()
```

---

## ⚡ 3. The 4-Tier Execution Architecture (Runtimes)

The `registry.rs` acts as the Universal Router. It dynamically hot-loads plugins and routes payloads based on the plugin's file extension, allowing limitless expansion.

1. **WASM Envelope (`wasmtime`):** 
   - **For:** Untrusted Community Plugins (Web Agents, Experimental Tools).
   - **Security:** Absolute memory sandboxing (`wasm_sandbox.rs`). It dynamically allocates fuel and memory limits based on system resources, completely eliminating the legacy 50MB hardcoded caps. Prevents rogue plugins from crashing the cluaiz Engine.
2. **Native C-FFI (`libloading`):** 
   - **For:** Trusted Core Plugins (e.g., `engine-lmdb` Database).
   - **Speed:** Zero-cost abstraction, direct memory mapping via the cluaiz Extension Protocol (CXP).
3. **Auto-WASM (`cargo` backend):** 
   - **For:** Developers writing raw `logic.rs` scripts. The Engine auto-compiles them into WASM in the background and hot-reloads them into RAM.
4. **Legacy Rhai (`rhai` interpreter):** 
   - **For:** Rapid prototyping and dynamic Python-like execution without compilation.

---

## 🛡️ 4. The CXP (cluaiz Extension Protocol) Boundary (`cxp_ffi.rs`)

Passing raw JSON strings across an FFI boundary causes massive CPU serialization bottlenecks and memory leaks. `inference-cel` completely drops the "JSON-only" mandate and introduces the **CXP Envelope**. 

```rust
#[repr(C)]
pub enum PayloadType {
    Json,
    Cdql,
    WasmBinary,
    RawBytes,
    Bincode, // Native Zero-Copy Binary Structs
}
```
Plugins define their "DNA / Genomes" as strict Rust structs. The Engine transpiles AST payloads into `Bincode` (Raw Bytes), allowing `0.05ms` cross-boundary execution speed without a single string allocation. The engine explicitly manages pointers across the FFI boundary using `cluaiz_free_payload` to guarantee zero memory leaks.

---

## 🧠 5. GPU Isolation & Crash Prevention (LAW 11 & 12)

**The Flaw:** If a dynamic `.dll` tries to manipulate VRAM (GPU memory) directly, it causes an OS-level Segmentation Fault.
**The CEL Solution:** The FFI layer enforces a strictly isolated pipeline. Plugins execute ONLY on the CPU and return data payloads (Pointers). The `gpu_injector.rs` then takes sole ownership of these payloads, validates them, and safely injects them into the LLM's Key-Value Cache/Context window using verified Sequence IDs. No plugin is ever allowed direct GPU access, guaranteeing engine stability.

---

## ⚙️ 6. The End-to-End Execution Pipeline Lifecycle

When the LLM outputs a CEL script, the following exact lifecycle occurs:

1. **Lexical Analysis (`lexer.rs`):** Uses a Recursive Descent Parser to read raw CEL strings, handle brace-matching `{ }`, identify semicolons `;`, extract variables, and parse structural filters (e.g., `>=`).
2. **Abstract Syntax Tree (`ast.rs`):** Stores the parsed logic as a hierarchical Turing-Complete tree of `CelStatement` (Blocks, Assignments, If/Else, Loops) and `CelOp` (Directives).
3. **Execution Planning (`planner.rs`):** The `CelPlanner` flattens the AST into a deterministic `ExecutionPlan` tree, mapping statements to `PlanBlock` and `PlanStep` nodes, separating "Fast Path" strings from complex FFI boundary crossings.
4. **Manifest Reading (`metadata_parser.rs`):** Reads `manifest.yaml` files parsing the strict 4-tier schema (`Discovery`, `Activation`, `Execution`, `Permissions`) to understand semantic triggers, lazy-loading rules, binary execution paths, and hardware constraints.
5. **Dispatch & Routing (`registry.rs`):** The router matches the `use plugin::<name>` directive to the loaded plugin.
6. **Execution (`cxp_ffi.rs` / `wasm_sandbox.rs`):** The payload is transpiled to a C-ABI struct and dispatched. Sandboxed modules run via Wasmtime, trusted modules run via native C-Pointers.
7. **GPU Injection & Native Execution (`gpu_injector.rs` / `cel_handler.rs`):** Returned payloads from plugins are verified and injected back into the LLM's attention mechanism. For `<cel>` hooks, the Native Engine Directives bypass plugins entirely and directly execute internal hooks (like KV cache flushing) inside the active inference loop.

> **"The Engine does not know what a Database is. It only knows how to speak CEL."** - cluaiz Engineering Doctrine
