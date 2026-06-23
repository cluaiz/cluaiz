# Cluaize Unified Build System (`cluaize-builder`)

Welcome to the **Unified Build System** for the Cluaize Inference Engine.

## ⚙️ How It Works (Zero Hardcoding)
This builder acts as an orchestrator. It does **not** hardcode any installation paths like `C:\Users\Aryan\.cluaize`. 
Instead, it dynamically links to `cluaize_shared::environment::EnvironmentManager` and asks the **Single Source of Truth** where the engine and drivers should go.

1. **Compiles the Workspace:** Builds the `cluaize.exe` kernel and APIs.
2. **Compiles the Drivers:** Triggers independent builds for `interface-engines/llama` and `onnx`.
3. **Resolves Paths:** Calls the Environment Manager to get `kernel_dir()` and `drivers_dir()`.
4. **Deploys:** Copies all `.exe` and `.dll` files to the perfectly resolved paths.

---

## 🚀 How to Run (Command Line Control)
You have full control over the build process via simple command-line flags. 

### Basic Run (Defaults to Dev + Release Optimized)
```bash
cargo build-all
```

### Full Control
You can pass the `--mode` and `--profile` arguments:
```bash
cargo build-all --mode <dev|public> --profile <debug|release>
```

#### 1. The `--mode` Flag
Controls **WHERE** the files are copied.
- `--mode dev`: (Default) Copies all files to a local `./.cluaize` folder inside the project. Safe for testing without breaking your main system.
- `--mode public`: Copies all files directly to your global production path (e.g., `C:\Users\Username\.cluaize`).

#### 2. The `--profile` Flag
Controls **HOW** the code is compiled.
- `--profile release`: (Default) Compiles with maximum optimizations (`cargo build --release`). Slower build, fastest runtime.
- `--profile debug`: Compiles without optimizations (`cargo build`). Faster build, good for quick syntax checks.

### Examples:
**1. Fast Local Dev Build:**
```bash
cargo build-all --mode dev --profile debug
```

**2. Full Production Global Release:**
```bash
cargo build-all --mode public --profile release
```
