# cluaiz Workflow: Build & Sync

This document provides a simple, deep, and 1:1 clear explanation of how to compile the cluaiz ecosystem and how to synchronize those compiled files into your local `.cluaiz` system folder.

The philosophy is strict:
1. **Building** only compiles the code. It does NOT touch your system files.
2. **Syncing** is a manual step. You explicitly tell the system when to update your production/dev `.cluaiz` environment with your newly compiled code.

---

## 🛠️ Step 1: Building (Compilation)

Use the `cluaiz-builder` to compile specific components. This gives you 1:1 granular control so you don't build "extra nonsense" you don't need.

> [!TIP]
> **Profiles:**
> Add `--profile release` for max performance (slow build, fast run). 
> Default is debug (fast build, slow run).

### 1. Build a Specific Single Driver
Builds *only* the requested hardware driver in isolation.
```bash
cargo run -p cluaiz-builder -- driver llama --profile <debug|release>
cargo run -p cluaiz-builder -- driver onnx --profile <debug|release>
```

### 2. Build All Drivers
Builds all available hardware drivers (Llama, ONNX, etc) without building the core engine.
```bash
cargo run -p cluaiz-builder -- drivers --profile <debug|release>
```

### 3. Build Core Engine
Builds *only* the core inference engine and CLI, ignoring the dynamic drivers.
```bash
cargo run -p cluaiz-builder -- core --profile <debug|release>
```

### 4. Build Entire Workspace (Everything)
Builds the Core Engine + CLI + All Drivers at once.
```bash
cargo run -p cluaiz-builder -- all --profile <debug|release>
```

---

## 🔄 Step 2: Synchronization (Deployment)

After building, your compiled `.dll` or executable files are sitting in `target/release/`. **They will NOT be used by the system automatically.**
To push these updates to your `~/.cluaiz` system folder, you must run the `dev-sync` command via the main CLI.

### 1. Sync Everything
Copies the newly compiled Core Engine, CLI, and All Drivers into `~/.cluaiz`.
```bash
cargo run -- dev-sync all --profile <debug|release>
```

### 2. Sync Core Only
Copies *only* the Core Engine (`engines.dll`) and CLI executable into `~/.cluaiz`, leaving your drivers untouched.
```bash
cargo run -- dev-sync core --profile <debug|release>
```

### 3. Sync All Drivers Only
Copies *only* the compiled drivers (LLaMA, ONNX, etc.) into `~/.cluaiz/engine/interfaces/`.
```bash
cargo run -- dev-sync drivers --profile <debug|release>
```

### 4. Sync a Specific Driver Only
Copies *only* the specific driver you specify. Highly useful when you only made a code change in one driver.
```bash
cargo run -- dev-sync driver llama --profile <debug|release>
cargo run -- dev-sync driver onnx --profile <debug|release>
```

---

## 🚨 Troubleshooting

If you run `cargo run` on a fresh system and get a **`Bootstrap Failed`** or **`os error 3`**, it means your `.cluaiz` folder is completely empty and missing the core engine files.
**Fix:** Run a full sync to populate it:
```bash
cargo run -- dev-sync all
```
