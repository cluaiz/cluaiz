# 🏛️ Independent CLI: Core Process Design
**Status**: DRAFT (Protocol Audit Phase)  
**Objective**: Industrial-grade process flow for the Archer Independent Console.

---

## 🔄 1. The Operational Lifecycle
This is the journey of the system from cold-boot to neural inference.

### 🏁 Step 1: System Ignition (`cargo run`)
- **Initialization**: The Kernel detects hardware (CPU/GPU) and loads the `SovereignProfile`.
- **View**: The TUI displays the **Isometric Logo** followed by the **Industrial Dashboard**.
- **Default State**: System enters `AppMode::Normal`. Focus is active on the **Industrial Input Buffer** (bottom).

### ⌨️ Step 2: Intent Expression (The Command Palette)
The console identifies intent via two semantic triggers:

#### A. Global Command Palette (`/`)
- **Trigger**: User types `/`.
- **Process**: 
    1. System shifts UI layer to `CommandPalette`.
    2. Overlays a selection menu: `[Roster, Settings, Help, Exit]`.
- **Interaction Bug Fix**: Currently, `Space` cancels the menu. The new process ensures that `Space` serves as the "Selection Confirmator" or "Argument Divider" without killing the UI state.

#### B. Context Injection (`@`)
- **Trigger**: User types `@`.
- **Process**: 
    1. System shifts UI layer to `ContextPalette`.
    2. Lists available hardware cores or project contexts.
- **Interaction Bug Fix**: Ensure seamless transition back to typing after selection.

---

## 🧭 2. Navigation & Feedback Logic

### 🖱️ Dashboard Interaction
- **Input Buffer**: A persistent, zero-latency buffer.
- **Navigation Controls**: 
    - `Up / Down`: Cycle through menu items or command history.
    - `Enter`: Execute the current buffer or selected menu item.
    - `Space`: **MUST** act as a standard character in the input buffer OR a selection toggle in menus. The current "Smart Dismiss" (Space = Exit) is a design flaw and will be purged.

### 🛰️ Real-Time Feedback (Telemetry)
- **Pulse Monitoring**: Every 100ms, the footer updates with:
    - **CPU/VRAM Load**: Real-time percentage bars.
    - **Independent Status**: `ACTIVE`, `IDLE`, or `INFERENCE`.
- **Inference Streaming**: When a model is running, tokens stream into the activity area with a real-time `TPS` counter.

---

## 🛠️ 3. Industrial Stability Protocols (The "No Khichdi" Rule)

### Event-Driven Rendering
- The system will **NOT** re-render on every tick.
- **Process**: 
    1. Input/Kernel event occurs.
    2. State is updated + `mark_dirty()` called.
    3. Terminal buffer is updated only for the affected Rects.

### Error Recovery
- If a model fails to load, the UI displays a `CRITICAL_ERROR` overlay with a "Deep Trace" option instead of crashing.
- `ESC` serves as the universal "Back" or "Reset State" key.

---
> [!IMPORTANT]
> The **Space Bar Conflict** is recognized as the primary UX failure. The overhauled process ensures that `Space` is context-aware and never destructive to the active UI mode.
