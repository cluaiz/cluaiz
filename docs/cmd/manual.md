# CLI Manual

The cluaiz CLI provides a set of direct terminal utilities to calibrate hardware, manage models, and run interactive generation.

---

## Core Command Set

The binary supports the following options:

| Command | Category | Description | Example |
| :--- | :--- | :--- | :--- |
| `cluaiz` | Core | Launches the interactive TUI Dashboard. | `cluaiz` |
| `cluaiz help` | Core | Displays command-line help screen. | `cluaiz help` |
| `cluaiz run <model-id>` | Models | Pulls and executes the specified model. | `cluaiz run bonsai:8b` |
| `cluaiz --calibrate` | System | Re-scans hardware limits and updates config profiles. | `cluaiz --calibrate` |
| `cluaiz --benchmark` | System | Executes a full hardware speed benchmark. | `cluaiz --benchmark` |

---

## Configuration JSON Assets

All CLI settings and commands are tracked in two local JSON profiles:

*   **`assets/commands.json`:** Mapped entries and command usage parameters, baked directly into the Rust binary at compile time.
*   **`Independent.json` / `system_control.json`:** Tracks active local configuration, hardware profiles, user identities, and loaded weight paths.
