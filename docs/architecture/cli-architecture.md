# 🏛️ Independent CLI Architecture (Industrial Design)
**Status**: DRAFT (Awaiting Approval)  
**Version**: 1.0.0 (Protocol Compliant)

## 🎯 Architectural Vision
The Archer Independent CLI is not just a terminal app; it's a **High-Performance Console** for the Independent Neural Engine. The goal is `0.0ms` UI latency and atomic state synchronization between hardware telemetry and neural inference.

---

## 🏗️ 1. Domain-Driven Separation (No Khichdi)
We will split the monolithic `AppState` into three distinct, decoupled domains:

### A. 🛡️ Kernel Domain (Logic)
- **NeuralEngine**: Direct bindings to CUDA/CPU runners via `engines` crate.
- **HardwareTelemetry**: Background thread polling Pulse (CPU/GPU/RAM) at 100ms intervals.
- **Orchestrator**: Manages background downloads and model hot-swapping.
- **Ownership**: Managed via an `Arc<Kernel>` for thread-safe access.

### B. 🎭 UI Domain (Transient State)
- **AppState**: Only contains UI-relevant state (e.g., `focused_element_id`, `scroll_offsets`, `active_tab`).
- **FlowEngine**: Handlers for rendering specific views (Dashboard, Chat, Roster).
- **Theme**: Centralized HSL-based styling system.

### C. 🛰️ Event Domain (Communication)
- **SovereignBus**: A tokio mpsc channel that streams `KernelEvent` (InferenceToken, DownloadUpdate, HardwarePulse) directly to the UI loop.

---

## 🔄 2. The Reactive Loop (Zero-Latency)
Instead of a manual `std::thread::sleep` loop, we use an event-driven `tokio::select!` orchestrator.

```rust
loop {
    tokio::select! {
        // 1. Hardware/Background Events
        Some(event) = kernel_rx.recv() => {
            state.apply_event(event);
            state.mark_dirty();
        }
        
        // 2. User Input (Keyboard)
        Some(key) = input_rx.recv() => {
            command_handler.dispatch(key, &mut state);
        }
        
        // 3. Rendering (Triggered only when Dirty or at 60fps)
        _ = render_interval.tick() => {
            if state.is_dirty() {
                flow.render(&state);
                state.clean();
            }
        }
    }
}
```

---

## 📱 3. Flow & Interaction Design (Post-Onboarding)

### A. The Dashboard (Command Center)
- **Hardware Visualizer**: Real-time per-core CPU usage + VRAM status.
- **Model Roster**: Interactive list of available neural weights with performance metrics.
- **Tabbed Experience**: Switching between "Chat", "Roster", "Telemitry", and "Settings" without re-initializing logic.

### B. The Command Palette (`/` and `@`)
- **Global Search**: `/` triggers a fuzzy-search menu across apps.
- **Context Injection**: `@` allows users to select specific hardware or model parameters for the next action.

### C. Inference Stream (Ghost Chat)
- **Streaming Tokens**: Direct UI buffer update for zero-latency text display.
- **Token Metrics**: TPS (Tokens Per Second) and Latency displayed in the footer in real-time.

---

## 🛠️ 4. Operational Execution
1. **State Partitioning**: Strictly move `Manual Logic` away from the CLI runner into the `Kernel` service.
2. **Atomic Views**: Every UI component must be a pure function `fn(state: &State, area: Rect, buffer: &mut Buffer)`. No side effects allowed.
3. **Zero-Warning Enforcement**: Mandatory `cargo clippy` and `cargo fmt` to reach internal Archer compliance.

---
> [!IMPORTANT]
> The Onboarding phase is considered "Frozen". This architecture focus is entirely on the **Independent Core** and **Dashboard UX**.
