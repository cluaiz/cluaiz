---
title: CEL Formatting and Embedding Rules
description: Why embedding CEL within JSON, YAML, or Markdown is an anti-pattern.
---

# CEL Embedding Rules

When architecting systems with cluaiz, developers often ask: *"Can I write CEL inside my JSON configuration file or a YAML manifest?"* or *"Can the engine parse CEL directly out of Markdown (`.md`) code blocks?"*

The architectural reality is **NO**. Embedding CEL inside serialized data structures or documentation files is a massive anti-pattern.

## 🛑 The JSON/YAML Anti-Pattern

```json
// BAD: Do not do this
{
    "config_name": "user_filter",
    "cel_logic": "let $x = use plugin::db -> invoke(get); $x -> filter(age > 18)"
}
```

### Why is this bad?
1. **Serialization Overhead:** To execute the CEL script, the host language must first parse the entire JSON object, extract the string value, and then pass it to the engine. This adds milliseconds of JSON parsing overhead.
2. **Loss of IDE Support:** By placing CEL inside a JSON string, your IDE treats it as a string. You lose all CEL syntax highlighting, auto-completion, and static linting.
3. **Escaping Hell:** Writing complex CEL scripts inside JSON strings requires escaping quotes (`\"`), which makes the code unreadable and prone to syntax errors.

---

## 🛑 The Markdown (`.md`) Reality

cluaiz **does not** execute CEL code natively from Markdown files. 

While the engine parses `SKILL.md` files (via `metadata_parser.rs`), it **only** extracts the YAML frontmatter bounded by `---` (for AI metadata like fuel limits and trigger words) and the human-readable instructions below it. It does **not** extract ````cel ```` code blocks to execute them.

```markdown
---
name: my-plugin
execution:
  envelope: "WASM"
---
# BAD: The engine will NOT execute this code block.
# It only reads the YAML above.
```cel
let $x = 1
```
```

---

## ✅ The Optimal Approach: Pure CEL

The `lexer.rs` in the cluaiz Engine is designed to parse **Pure CEL Strings**. 

The absolute most optimal way to execute CEL is:
1. **Direct String via SDK:** Write the pure CEL string inside your host language (Go/Python/C++) and pass it directly to the FFI pointer.
2. **Dedicated `.cel` Files:** Save the CEL logic in a dedicated file (e.g., `logic.cel`) and have your host application read the file into memory and pass the string to the SDK. 

**Example (Node.js reading a `.cel` file):**
```javascript
const fs = require('fs');
const cluaiz = require('cluaiz-sdk');

// Slower (JSON overhead)
// const config = JSON.parse(fs.readFileSync('config.json'));
// cluaiz.execute(config.cel_logic);

// Optimal (Pure CEL)
const pure_cel = fs.readFileSync('logic.cel', 'utf8');
cluaiz.execute(pure_cel);
```

By keeping CEL in its native format, you ensure maximum execution speed and maintain clean, readable codebases.
