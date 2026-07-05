# CEL Syntax Matrix — The Universal Execution Reference

> *"चाहे WASM plugin हो, या direct C++ GPU call — सब कुछ CEL AST के ज़रिये map होता है।"*

This single page is the exhaustive, exact reference for every operator and control flow supported by the cluaiz Engine parser (`inference-cel/src/parser/ast.rs`). 

---

## 🗺️ The Complete Syntax Matrix

### 1. Core Data Flow & Pipelines

| CEL Syntax | AST Enum | Under the Hood (Engine Level) |
|---|---|---|
| `->` | `Pipe` | Memory barrier. Passes the binary `ExtensionPayload` struct to the next node. |
| `use plugin::<name>` | `ImportPlugin` | Checks `EngineRules`. Dynamically links `.dll/.so` (via `libloading`) with path traversal guards, or loads WASM into global `WASM_CACHE`. |
| `invoke(<method>, args...)` | `InvokeAction` | Translates args to FFI boundary. For WASM, strictly enforces `wasmtime::Store::set_fuel` and `ResourceLimiter` before execution. |
| `process("<text>")` | `FastProcess` | Bypasses VM initialization. Triggers fast-path CPU loops (SIMD optimization) for raw string manipulations. |

---

### 2. Query & Data Transformation

| CEL Syntax | AST Enum | Under the Hood (Engine Level) |
|---|---|---|
| `<action> <Target>(<args>)` | `Command` | Generic parser fallback. Matches `action target(args)` and passes them dynamically to the executor plugin payload. |
| `filter <field> <op> <val>` | `Filter` | Performs native CPU comparisons (`>`, `<`, `==`, `!=`, `>=`). Drops unmatched memory payloads *before* they are sent to the next node. |
| `select(<f1>, <f2>)` | `Select` | Memory projection. Strips unused fields from the `ExtensionPayload` to prevent VRAM footprint leaks during massive dataset iterations. |
| `time_window(size: "1h")` | `TimeWindow` | Hard-truncates the active session memory/context for token generation loops to stay within strict KV Cache limits. |
| `similar_to(vec: [...], metric: "L2")` | `SimilarTo` | Dispatches direct hardware vector similarity scans (Cosine, L2, Dot) directly to CPU SIMD registers or GPU tensor cores. |

---

### 3. Hardcore Engine Directives (Phase 3 Ecosystem)
These are reserved syntaxes that bypass external plugins and directly interact with the host Engine hardware and scheduler.

| CEL Syntax | AST Enum | Under the Hood (Engine Level) |
|---|---|---|
| `engine -> kv_cache -> clear($user)` | `EngineMemoryControl` | Triggers atomic reclamation of GPU VRAM allocated to the selected user's attention layers. Zero-latency memory free. |
| `engine -> mid_layer -> inject($d)` | `MidLayerInjection` | Bypasses standard token generation loops to inject structured memory strings directly into the Transformer's attention mechanisms. |
| `engine -> inference -> pause()` | `InferenceControl` | Signals the `tokio` generation thread to yield compute cycles to higher priority tasks. |
| `engine -> os -> process("ps")` | `SystemCall` | Spawns host OS subprocesses. Only executes if `allow_subprocess: true` in `EngineRules` manifest. |

---

### 4. Control Flow (Turing Complete Statements)
CEL isn't just a query language; it handles state and branching directly inside the execution thread, saving round-trips to the AI Agent.

| CEL Statement | AST Struct | Under the Hood (Engine Level) |
|---|---|---|
| `let $var = <Pipeline>` | `Assignment` | Allocates a `CelValue` inside the current execution frame's hashmap (`Variable` type) to avoid recomputing expensive API/Plugin calls. |
| `if (<cond>) { ... } else { ... }` | `IfElse` | Evaluates native Rust booleans. Bypasses the AST branch that fails, preventing memory allocation for that branch. |
| `foreach ($item in $list) { ... }` | `Foreach` | Native `while let` loop over `CelValue::Vector` or arrays. Re-uses the exact same WASM linear memory block for each iteration. |
| `config(<key>="<value>", ...)` | `Config` | Internally triggers the Settings Controller to dynamically update the YAML manifest of the loaded extension. (e.g., `use extension::cluaiz-search -> config(search_api_type="tavily")`) |

---

## 💾 AST Value Types (`CelValue`)
When transpiled to Bincode, JSON types are strictly mapped to Rust memory layouts:

| CEL Type | Syntax Example | Rust Memory Mapping (`CelValue`) | Description |
|:---|:---|:---|:---|
| **Text** | `"Hello"` | `CelValue::Text(String)` | Standard UTF-8 String slice. |
| **Number** | `42.5` | `CelValue::Number(f64)` | 64-bit float precision. |
| **Bool** | `true` | `CelValue::Bool(bool)` | 1-byte boolean flag. |
| **Vector** | `[0.1, 0.9]` | `CelValue::Vector(Vec<f32>)` | Dense 32-bit floats for high-dimensional ML embeddings. |
| **Variable**| `$user_profile`| `CelValue::Variable(String)` | Hashmap environment key lookup pointer. |
| **Null** | `null` | `CelValue::Null` | Zero-allocation struct placeholder. |

--- 

## 🚨 Common Mistakes & How to Avoid Them

| `use plugin::catalog -> invoke(get_all) -> similar_to() -> filter status == "active"` | `use plugin::catalog -> invoke(get_active) -> similar_to()` | **OOM Error:** Running a vector scan on a million rows before filtering by status destroys VRAM. Always pre-filter natively or inside the plugin. |
| Executing native shell `engine -> os -> process("ls")` | `use plugin::file_system -> invoke(list)` | **Security Denied:** `SystemCall` requires explicit `allow_subprocess` in manifest. Use designated WASM plugins instead. |
| `let x = use ...` (inside a `foreach`) | `let x = use ...` (outside), then `foreach` | **CPU Thrashing:** Repeatedly invoking a plugin inside a loop re-triggers WASM pointer parsing. Assign to a variable first. |

---

### ⚙️ Component Configuration Macros (Settings Injection)

CEL scripts and agents can dynamically reprogram components in real-time during execution using the `config(...)` macro. This macro transpiles down to an AST node that calls the Engine's Universal Settings Controller.

| Domain/Component Type | CEL Syntax Example | What happens internally |
|---|---|---|
| **Extension** | `use extension::cluaiz-search -> config(search_api_type="tavily", search_api_key="xxx")` | Stores overrides safely in `user_settings.yaml`, leaving the schema manifest intact. |
| **MCP (Server)** | `use mcp::github-connector -> config(permissions.network_access=true)` | Updates `user_settings.yaml` to grant network access on the fly without mutating the manifest. |
| **Plugin (WASM)** | `use plugin::pdf-parser -> config(settings.max_threads=4)` | Modifies `user_settings.yaml` to allocate more threads before execution. |
| **Skill (Markdown)** | `use skill::code-reviewer -> config(settings.strict_mode=true)` | Safely stores the setting override in `user_settings.yaml`. |

> **Note:** The `config()` macro executes *before* the component is initialized, ensuring that any subsequent `invoke()` calls use the freshly updated settings/permissions!
