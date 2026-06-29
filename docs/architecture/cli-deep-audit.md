# 🏛️ Independent CLI Deep Audit (Research)
**Component**: Archer Independent Console (`cluaiz-ai-/cli`)  
**Date**: 2026-04-20  


## 🎯 Internal Gap Analysis (The "Khichdi" Audit)
The current implementation of the CLI suffers from extreme **Domain Leakage** and **Monolithic State Decay**.

### 1. The "God State" Problem
- **File**: `cli/src/core/state.rs`
- **Issue**: `AppState` contains 149 lines of struct definition, mixing:
    - **Hardware State**: `cpu_usage`, `ram_gb`, `silicon_info`.
    - **Logic State**: `OsState`, `NeuralEngine`, `DownloadProgress`.
    - **Transient UI State**: `roster_h_scroll`, `details_btn_idx`, `palette_state`.
- **Impact**: Any change to UI logic triggers a full recompile/re-eval of core state. Impossible to test in isolation.

### 2. Manual Tick/Frame Loop
- **File**: `cli/src/core/app.rs`
- **Issue**: `App::run` uses a `while` loop with `std::thread::sleep(16ms)`. 
- **Impact**: This is "Pseudo-Asynchronous". It blocks the thread and doesn't leverage Tokio's full potential for a high-performance console. Flicker-free atomic rendering is impossible with this "game loop" approach if the system lags.

### 3. Logic-UI Coupling
- **Issue**: `OnboardingEngine` and `DashboardEngine` are tucked inside `core/`, but they directly interact with `AppState` which is full of UI markers.
- **Inertia**: The logic is not "Independent" (independent); it is tied to the `ratatui` widgets.

---

## 🌎 External Truth-Grounding (`gemini-cli-main`)
Auditing the `Other/gemini-cli-main` architectural blueprint reveals how an industrial-grade CLI should look.

### 1. Modular Domain Decoupling
- **Ref**: `packages/core/src/`
- **Pattern**: Separate domains for `telemetry`, `scheduler`, `billing`, and `config`.
- **Difference**: They use a "Service-Consumer" model. The CLI is just one consumer of the `core` services.

### 2. Event-Driven Architecture
- **Ref**: `confirmation-bus`, `telemetry`
- **Pattern**: Asynchronous event streams instead of manual state polling.
- **Independent Path**: Replace manual `mark_dirty()` with a reactive event bus.

---

## 🛠️ Independent Recommendations (Path to 0.0ms Latency)
1. **State Partitioning**: 
    - Move Hardware/Neural logic to a `KernelState`.
    - Keep `UiState` local to rendering components.
2. **Atomic Rendering Engine**: Use a dedicated `FlowEngine` upgrade that separates "Data Snapshot" from "Terminal Print".
3. **Zero-Warning Enforcement**: Clean all 50+ warnings in `engines` and `llama` crates to ensure memory safety.
4. **Task Orchestration**: Use `tokio::select!` for event handling instead of a sleeping loop.

---
> [!IMPORTANT]
> This audit confirms that the current "Khichdi" architecture is a fatal flaw for a 10M+ scale industrial console. Architectural lockdown is required.
