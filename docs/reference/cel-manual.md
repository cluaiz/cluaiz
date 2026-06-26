# CEL (Cluaize Expression Language) Specification & Reference Manual

Cluaize Expression Language (CEL) is the Turing-complete orchestration DSL utilized by the Cluaize Inference Engine. It acts as the single bridge language connecting AI actions, dynamic skills, state variables, and low-level engine parameters across sandboxed execution boundaries.

---

## 1. Syntax Rules

All CEL statements fall into one of four primary grammar blocks: expressions, assignments, conditional branches, or iterations.

### A. Core Pipelines & Operators

A CEL statement is executed as a pipeline chain using the `->` operator. Each segment of the pipeline corresponds to a specific AST operation in [CelOp](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L38-L114).

```text
use plugin::database -> find User(id: 42) -> select(username, email)
```

### B. Variable Assignments (`let`)

Saves state intermediate results inside the engine memory to prevent expensive round-trips to the LLM agent.

```text
let $result = use plugin::scrapper -> extract(url: "https://cluaize.com");
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

## 2. AST Value Types (`CelValue`)

The [CelValue](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L5-L14) enum strictly dictates the data layout types:

| Type | Syntax Example | Serialized Output |
|:---|:---|:---|
| **`Text`** | `"Hello World"` | UTF-8 String slice |
| **`Number`** | `42.5` | `f64` Float precision |
| **`Bool`** | `true` | `bool` Boolean |
| **`Vector`** | `[0.1, -0.2, 0.9]` | `Vec<f32>` High-dimensional embeddings |
| **`Variable`** | `$user_profile` | Evaluated key lookup |
| **`Null`** | `null` | Zero allocation unit |

---

## 3. Reference: Core Operators (`CelOp`)

Below is the exhaustive specification for each instruction option defined in [CelOp](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L38-L114).

* ### `use plugin::<name>`
  * **Syntax:** `use plugin::<name>`
  * **Action:** [ImportPlugin](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L40)
  * **Behind the Scenes:** Checks the package registry, resolves paths using path canonicalization guards, parses the plugin manifest `SKILL.md`, and dynamically loads the WASM module into the Store cache.

* ### `invoke(<method>, args...)`
  * **Syntax:** `use plugin::auth -> invoke(verify, token: "xyz")`
  * **Action:** [InvokeAction](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L44)
  * **Behind the Scenes:** Resolves FFI parameters from the payload vector and triggers the WASM execution hook. Enforces fuel limits and memory caps defined in the metadata.

* ### `filter`
  * **Syntax:** `-> filter age > 18`
  * **Action:** [Filter](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L51)
  * **Behind the Scenes:** Performs native binary filters in Rust on the stream dataset using [CompareOp](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L18-L26) branches before allocating memory for downstream tasks.

* ### `process(<text>)`
  * **Syntax:** `process("Raw Input text")`
  * **Action:** [FastProcess](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L58)
  * **Behind the Scenes:** Triggers fast-path CPU loops bypassing heavy VM compiler initialization overhead when processing non-structured string conversions.

* ### `select`
  * **Syntax:** `-> select(username, email)`
  * **Action:** [Select](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L87)
  * **Behind the Scenes:** Projects and strips unused fields from serialized payload envelopes to prevent memory footprint leaks during massive dataset iteration loops.

* ### `similar`
  * **Syntax:** `-> similar_to(vector: [...], metric: "cosine")`
  * **Action:** [SimilarTo](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L74)
  * **Behind the Scenes:** Dispatches similarity scans directly to the CPU SIMD registers or GPU vector cores.

* ### `time_window`
  * **Syntax:** `-> time_window(size: "1h")`
  * **Action:** [TimeWindow](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaize/inference-cel/src/parser/ast.rs#L69)
  * **Behind the Scenes:** Configures historical memory context limits to keep generation loops within bounds.

---

## 4. Hardcore Engine Control Directives

These commands bypass typical extensions to inject state commands directly into the core runtime scheduler.

### A. KV Cache Control
```text
engine -> kv_cache -> clear($user_id)
```
Triggers atomic reclamation of GPU memory allocated to the selected user session's attention layers.

### B. Middle-Layer Injection
```text
engine -> mid_layer -> inject($data)
```
Bypasses the normal token prediction loops to inject contextual information directly into the attention layers.

### C. Inference Scheduling Control
```text
engine -> inference -> pause()
```
Forces the tokio generation threads to yield compute cycles to higher priority tasks.

### D. OS System Execution
```text
engine -> os -> process("ps")
```
Spawns subprocesses to gather host monitoring metrics (requires `allow_subprocess` permissions configured inside the model manifest).
