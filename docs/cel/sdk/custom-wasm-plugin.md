---
title: Building Custom WASM Plugins
description: How to compile standalone tools into WebAssembly plugins for the cluaiz Engine.
---

# Building Custom WASM Plugins

While your main application (Chat App, Website) remains in your preferred host language (Python, Node.js), you can write specialized, isolated tools and compile them into **WebAssembly (WASM)**.

When the cluaiz Engine executes a WASM plugin, it bypasses traditional FFI overhead because the WASM code runs *inside* the Engine's linear memory. This provides nanosecond-level "supercharged" execution speed.

---

## When to Compile to WASM?

> [!TIP]
> **DO NOT** compile your entire Chat Application or Website backend into WASM.

**Use Pure CEL (No Compilation):**
For basic 1-to-2 line logic routing data between existing plugins (e.g., passing a query to a Vector DB and then to an LLM).

**Compile to WASM:**
When you are building a custom, isolated tool that requires raw CPU speed or offline processing. Examples:

- A custom mathematical parser or physics engine.
- A proprietary token counting algorithm.
- An offline OCR or image processing script.
- Complex data transformation that pure CEL declarative syntax cannot express easily.

---

## Compiling Languages to WASM

You do not have to write plugins in C or Rust. Modern toolchains allow you to compile almost any language into a standalone `.wasm` module.

### 1. Rust (The Native Choice)

Rust is the fastest and most seamless way to build plugins since the engine itself is written in Rust.

```bash
cargo build --target wasm32-wasi --release
```

```rust
// A basic Rust WASM Plugin
#[no_mangle]
pub extern "C" fn cluaiz_entry() {
    let output = b"Hello from Rust WASM";
    unsafe { cluaiz_host_return(output.as_ptr(), output.len()) };
}
```

### 2. JavaScript / TypeScript (AssemblyScript)

If you are a Node.js developer, you can write TypeScript-like syntax and compile it directly to WASM using [AssemblyScript](https://www.assemblyscript.org/).

```bash
npm run asbuild:release
```

```typescript
// assembly/index.ts
export function cluaiz_entry(): void {
  // Logic here
}
```

### 3. Go (TinyGo)

Standard Go binaries are large, but you can use [TinyGo](https://tinygo.org/) to create highly optimized, small WASM plugins.

```bash
tinygo build -o plugin.wasm -target=wasi main.go
```

```go
// main.go
package main

//export cluaiz_entry
func cluaiz_entry() {
    // Logic here
}
func main() {}
```

### 4. Python / C++

- **C/C++**: Compile using Emscripten (`emcc`).
- **Python**: You can use Pyodide or MicroPython to bundle your script into WASM, but note that the `.wasm` file will be heavy (~10MB) because it must bundle the Python interpreter. Only do this if absolutely necessary; otherwise, rewrite the tool in Go or Rust.

---

## 4. Step-by-Step: Grouping & Registering Custom Plugins

If you build 10-15 different WASM tools, you must bundle them into a single **Plugin Package** (a Master File architecture) so the Engine can load them efficiently.

Here is the exact step-by-step guide to registering your own custom WASM files locally.

### Step 1: Create the Folder Structure
Create a dedicated folder on your disk. Place all your compiled `.wasm` files inside a `bin/` subdirectory.

```text
/opt/cluaiz/plugins/my-custom-tools/
├── manifest-plugin.yaml    <-- The Master Execution File
└── bin/
    ├── math_tool.wasm
    ├── string_tool.wasm
    └── image_tool.wasm
```

### Step 2: Write the Master `manifest-plugin.yaml`
This file tells the engine exactly what your plugin does, its security limits, and how to route requests to your different `.wasm` files.

> [!TIP]
> **Complete Example:** View a fully documented, real-world example of this file here: [**`docs/cel/manifest-plugin.yaml`**](file:///c:/Users/Aryan/my/Cluaiz-workspace/Cluaiz-Technologies/cluaiz/docs/cel/manifest-plugin.yaml).

Create `/opt/cluaiz/plugins/my-custom-tools/manifest-plugin.yaml`:

```yaml
name: my-custom-tools
version: 1.0.0
description: A collection of internal WASM tools for our Chat App.

permissions:
  max_memory_mb: 128
  max_cpu_time_ms: 10000

execution:
  envelope: "WASM"
  # Advanced Execution Routing for Multiple WASM files
  routes:
    - method: "calculate"
      binary_path: "bin/math_tool.wasm"
      entry_point: "cluaiz_entry"
    - method: "parse_string"
      binary_path: "bin/string_tool.wasm"
      entry_point: "cluaiz_entry"
    - method: "process_image"
      binary_path: "bin/image_tool.wasm"
      entry_point: "cluaiz_entry"
```

### Step 3: Register the Plugin in the Engine
The Engine will not scan your hard drive. You must explicitly tell the Engine where your master folder is by adding it to the `registry.yaml`.

Open `~/.cluaiz/engine/config/registry.yaml` and append your plugin:

```yaml
extensions:
  - id: ext_my_custom_tools
    domain: /opt/cluaiz/plugins/my-custom-tools/
    load_strategy: LAZY
    enabled: true
```
*Note: `LAZY` loading ensures the 128MB memory cap isn't allocated until the CEL script actually calls the plugin.*

### Step 4: Invoke the Tools via CEL
Now, your host application (Python/Node.js) can instantly trigger any of the 15 tools inside your master plugin package. The Engine automatically matches the `invoke()` method to the correct `.wasm` file based on your `manifest-plugin.yaml` routes.

```cel
// 1. Call the math_tool.wasm
let $math_result = use plugin::my-custom-tools -> invoke(calculate, data: "2+2")

// 2. Pass the result directly into string_tool.wasm (Zero Host Overhead)
let $final = use plugin::my-custom-tools -> invoke(parse_string, input: $math_result)
```

To understand how the Engine automatically discovers your plugin folder and reads the `manifest-plugin.yaml` to load the WASM files, read the [Manifest Architecture Guide](./manifest-architecture.md).
