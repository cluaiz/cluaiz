# 🧠 cluaiz Source Architecture (`cmd/src`)

Welcome to the central nervous system of the cluaiz ecosystem. This directory contains the sovereign source code for the Command Line Interface (CLI) and the immersive Ratatui-based Terminal User Interface (TUI). 

The `cmd` package is **NOT** just a wrapper. It acts as the OS-level orchestrator, managing boot lifecycles, hardware verification, background engine daemons, and user interactions.

---

## 📂 Directory Structure & Modules

### `main.rs`
The absolute entry point of the entire system.
- **Sovereign Ghost Execution Guard:** Ensures the binary is running from the master `~/.cluaiz/bin` path.
- **CLI Argument Parser:** Uses `clap` to handle one-off commands (e.g. `cluaiz dev-sync`, `cluaiz brain on`).
- **TUI Initialization:** If no CLI args are provided, it mounts the terminal canvas and launches the `App` flow.

### `core/` (The Brainstem)
This module dictates state management and system lifecycle.
* **`bootstrapper.rs`**: The heartbeat of cluaiz. 
  * Checks for missing components.
  * Handles artifact synchronization (e.g., `dev-sync`).
  * Fetches `package.json` to verify engine versions.
  * *Law of the Engine:* cluaiz cannot launch until the bootstrapper successfully seals the registry.
* **`app.rs`**: The main Ratatui loop. Handles terminal event ticks (keyboard inputs) and orchestrates view rendering.
* **`state.rs`**: Contains the global `OsState` machine (Onboarding -> MainMenu -> Dashboard, etc) and User states.
* **`dashboard.rs` / `onboarding.rs`**: High-level page orchestrators for the UI logic.
* **`flow.rs`**: Manages terminal context switching (raw mode, screen clearing, crash recovery).

### `ui/` (The Presentation Layer)
Contains all the highly customized, dynamic Ratatui components.
* Uses glassmorphism effects, dynamic colors, and smooth micro-animations (via `tachyonfx`).
* Components are modularized so `dashboard.rs` can plug and play elements like the Side Menu, Chat Interface, or Hardware Monitors.

### `cli/` (Headless Commands)
Code for executing specific command-line arguments without starting the TUI.
* Example: `ps.rs` (process status), `test_jit.rs` (cache benchmarks).

---

## ⚙️ How Booting Works (The cluaiz Lifecycle)

When you type `cluaiz` in your terminal:

1. **Ignition (`main.rs`)**: 
   The CLI checks if it's running from the correct global path. It then parses args.
2. **Bootstrapper Validation**:
   `Bootstrapper::ignite()` verifies that the Neural Foundry (permissions) and Skills exist. It ensures `~/.cluaiz/engine` is populated.
3. **Flow Engine Engaged**:
   The terminal enters Raw Mode. Alternate screens are prepared.
4. **App Mount**:
   `App::new()` is called, state is restored from local storage. If it's a first-time launch, you are sent to `OsState::Onboarding`. Otherwise, you land in the `Dashboard`.
5. **Render Loop**:
   The program runs an asynchronous `tokio` event loop. It listens for `crossterm` events (key presses) and updates the UI state at 60 FPS.

---

## 🛠️ Development Rules for `cmd/src`

1. **Never block the event loop:** Any heavy task (like loading a model or talking to the API) MUST be done asynchronously using `tokio::spawn` or background channels.
2. **Graceful Crashes:** If something panics, the `FlowEngine::restore()` must be called via the panic hook in `main.rs` to prevent the user's terminal from breaking.
3. **No Direct Hardware Calls here:** The CLI should always use `cluaiz-shared` or the `engines` API crate to talk to the GPU/Drivers. The UI must remain decoupled from the hardware execution logic.
