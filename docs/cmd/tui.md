# TUI Dashboard

The cluaiz command center is an interactive terminal user interface (TUI) optimized for high responsiveness and zero UI flickering during heavy tensor operations.

---

## State Machine Architecture

The TUI maintains a distinct, dual-dimensional state machine to manage user context and system lifecycle:

*   **OsState (Lifecycle):**
    *   `Setup(OnboardingStep)`: Manages the onboarding steps (Identity, Purpose, Silicon Audit).
    *   `Dashboard`: The main application grid once node configurations are finalized.
*   **MenuApp (Context):**
    *   `Roster`: Displays model catalogs, local cached weights, and disk allocation.
    *   `Settings`: Configures runtime parameters and displays system telemetry.
    *   `Chat`: Handles real-time conversation and text generation loops.

---

## Asynchronous Communication Channels

To prevent the terminal UI from freezing or lagging during massive mathematical calculations, cluaiz decouples rendering from data processing:

*   **Dedicated Tokio Threads:** Inference loops and network downloads run entirely in separate background threads managed by `tokio`.
*   **MPSC Sockets:** Telemetry data, generation tokens, and progress statuses are streamed to the interface thread using multi-producer, single-consumer (`mpsc`) message channels.
*   **Visual Precision:** Ratatui blocks are rendered at a stable frequency, using fixed padding sizes to eliminate layout shifting during text generation.
