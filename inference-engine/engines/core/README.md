# 🌐 Archer Shared Crate: The Unified Architectural Fabric

Welcome to **`archer-shared`** — the core network system and structural backbone of the entire **cluaiz AI  Ecosystem**. 

Unlike standalone scripts, `archer-shared` is not an execution layer. It is a strictly controlled, universally decoupled **Core Shared Library** (`lib.rs`). Its purpose is to enforce absolute Architectural Truth across all disparate compute engines (like `archer-candle` and `archer-llama`), ensuring they speak the exact same language without ever duplicating code.

---

## 🏗️ Directory Macro-Architecture

The crate is rigorously divided into dedicated behavioral domains to enforce the Single Responsibility Principle (SRP).

### 1. `backend/` (The Extension Interfaces)
This is the protocol layer that defines *how* any external Neural Engine must talk to the cluaiz UI. 
- **`traits.rs`**: Defines the rigid traits (like `EngineBackend` or `InferenceEngine`) that `llama-cpp` and `Candle` must implement. This ensures the main Orchestrator can blindly hot-swap engines depending on hardware.
- **`signature.rs`**: The handshake rules. It guarantees the returned tensors and logging metrics fit the global UI stream perfectly.
- **`context.rs`**: Agnostic KV-cache / context memory mappings avoiding library-specific lock-ins.

### 2. `hardware/` (The Agnostic Silicon Probes)
**Rule:** Strict OS-Isolation and Non-Hardcoded Mathematical Telemetry.
- Contains the `SiliconProvider` trait binding `windows_sensor.rs`, `linux_sensor.rs`, and OS abstractions. 
- Responsible for measuring native memory bandwidth (`benchmark.rs`), enforcing execution feasibility limits (`speed_checker.rs`), and returning 100% accurate, dynamically probed `SovereignProfile` hardware metadata without blindly relying natively on strings like "CUDA".
- **Documentation:** See `hardware/ReadMe.md` for extreme depth on this specific domain.

### 3. `neural_core/` (Mathematical Control Structure)
Defines the universal math boundaries independent of the backend.
- **`config.rs`**: Standardized configurations for Hyperparameters (Context Size, Temperature, Top-K, Repetition Penalties). 
- **`sampling.rs`**: Centralizes the logic for how tokens are accepted or rejected to minimize hallucination (Sampler interfaces).
- **`ops/`**: Low-level operational structs mapping memory layouts or chunk-streams. 

### 4. `metadata/` (The Independent Registry)
The parsing mechanism for reading model capabilities from disk safely.
- **`dna.rs`**: Contains the `StructuralDNA` and `SovereignProfile` definitions. It allows the system to read a model's intrinsic physical architecture before risking an OOM (Out-of-Memory) load.
- **`manifest.rs`**: The mapping for HuggingFace assets and parameter schemas.

### 5. `prompting/` (The Linguistic Templater)
Instead of relying on fragile prompt concatenation, this module manages universal templating systems (`templater.rs`).
- Ensures that prompts sent to a Gemma model get exactly `<start_of_turn>user` formatting, while prompts destined for Llama 3 get `<|start_header_id|>`. The  CLI does not worry about format; this layer secures the structure invisibly.

### 6. `orchestrator.rs` (The Grand Conductor)
This is the singular entry point where everything collides perfectly.
The Orchestrator receives:
1. The user's requested Model (`metadata`).
2. The current physical capability limit (`hardware`).
3. The prompt request (`prompting`).

It evaluates these, calculates the exact mathematical limits, and seamlessly activates the correct backend (Llama/Candle) via traits defined in `backend/`. If a failure occurs, the Orchestrator gracefully downshifts natively instead of crashing the system.

---

## 🔗 The Golden Rules of `archer-shared` (CTO Protocol)

1. **DRY (Don't Repeat Yourself):** If you are writing OS-logic checking `vram_gb` in `speed_checker` AND in `windows_sensor`, you have failed. Data is extracted *once* in its assigned folder and cascaded as variables.
2. **Framework Decoupling:** `archer-shared` must NEVER contain traits or functions imported from PyTorch, Candle, or Llama.cpp directly. It is a completely pure Rust generic fabric. External engines depend on *this*, not the other way around.
3. **Ghost Telemetry:** All shared state mutations must be lock-free atomic processes to prevent the CLI dashboard from stalling inference cycles.
