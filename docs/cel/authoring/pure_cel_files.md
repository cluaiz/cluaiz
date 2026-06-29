---
title: Authoring Native .cel Files
description: How the engine handles pure .cel files, and the benefits of native authoring.
---

# Authoring Native `.cel` Files

While you can pass CEL strings directly into the Native SDK from your Python or Node.js code, hardcoding massive multi-line strings in your host language can quickly become unmanageable.

The most scalable, optimal, and developer-friendly approach is to author your logic in dedicated **`.cel` files** (e.g., `pipeline.cel`) and have your host application read those files into the SDK.

## How the Engine Handles `.cel` Files

When a `.cel` string is passed to the engine, the internal architecture processes it as follows:

1. **`lexer.rs` Tokenization:** The engine scans the raw string and breaks it down into syntactic tokens (keywords, variables, operators).
2. **AST Construction:** It builds an Abstract Syntax Tree (AST), checking for strict structural correctness.
3. **Execution Routing:** It determines if the operations require WASM plugins, Rhai scripts, or native Rust calls, and pipelines the data accordingly.

### The Power of Parallel Parsing (Cold Boot)
If you have 50 different CEL pipelines, keeping them in `.cel` files allows your host application to read and parse all of them asynchronously during application startup (Cold Boot). 
Instead of waiting to parse a string *during* an API request, the engine can pre-compile the ASTs into memory, resulting in instant execution when a request arrives.

## Benefits of Native `.cel` Authoring

Why separate `.cel` files instead of strings in Python/JSON?

### 1. IDE Syntax Highlighting & Tooling
When you write code inside a Python string or a JSON payload, the IDE sees it as raw text. 
By using a `.cel` file, your IDE (like VS Code) can provide native syntax highlighting, bracket matching, and code formatting. This drastically reduces typographical errors.

### 2. Compiler Safety & Linting
Because `.cel` files are isolated, future CLI tools (like `cluaiz check pipeline.cel`) can statically analyze the file for syntax errors or invalid plugin references *before* your backend even starts running.

### 3. Clean Separation of Concerns
Your Python/Node.js code should only handle HTTP routing and business logic. The heavy AI pipelining, vector search, and data transformation belong in the `.cel` file. 

**Bad (Hardcoded String):**
```python
# Unreadable, no highlighting, hard to maintain
script = "let $x = use plugin::vision -> invoke(ocr, image: ?1); if ($x != '') { use plugin::llm -> invoke(summarize, text: $x) } else { 'NO_TEXT' }"
```

**Optimal (Native `.cel` File):**
```cel
// pipeline.cel - Full highlighting, comments, and readability
let $image = ?1

let $x = use plugin::vision -> invoke(ocr, image: $image)

if ($x != "") {
    use plugin::llm -> invoke(summarize, text: $x)
} else {
    "NO_TEXT"
}
```

```python
# main.py - Clean host code
import cluaiz_sdk
import aiofiles

async def process_image(img_bytes):
    # The string is read once at startup or dynamically, but the Python code stays clean.
    async with aiofiles.open('pipeline.cel', mode='r') as f:
        pure_cel = await f.read()
        
    return cluaiz_sdk.execute(pure_cel, [img_bytes])
```

## When to write `.cel` files?
- **Always**, unless the CEL logic is trivially small (e.g., a simple 1-liner like `use plugin::db -> invoke(get)`). 
- If your CEL logic contains `if/else` branches, `foreach` loops, or spans more than 3 lines, it belongs in a native `.cel` file.
