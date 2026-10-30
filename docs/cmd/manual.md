# CLI Manual

The cluaiz CLI provides a set of direct terminal utilities to calibrate hardware, manage models, and run interactive generation.

---

## Core Command Set

The binary supports the following options:

| Command | Category | Description | Example |
| :--- | :--- | :--- | :--- |
| `cluaiz` | Core | In a terminal with a default chat model: the agent in the current folder. Otherwise the help. | `cluaiz` |
| `cluaiz --help`, `cluaiz <command> --help` | Core | Every command and flag. | `cluaiz run --help` |
| `cluaiz -v`, `-V`, `--version` | Core | The version. | `cluaiz -v` |
| `cluaiz run [model] ["prompt"]` | Models | Chat in the terminal, or answer one prompt ([guide](chat.md)). | `cluaiz run qwen3 "hi"` |
| `cluaiz agent ["task"]` | Agent | Coding agent in a folder: reads, searches, edits files, runs commands, asks before changes ([guide](agent.md)). | `cluaiz agent` |
| `cluaiz calibrate` | System | Re-scans the hardware after a hardware or driver change. | `cluaiz calibrate` |
| `cluaiz bench <model>` | System | Prompt and generation speed (llama-bench). | `cluaiz bench qwen3` |

The full list is in the [command reference](commands_reference.md).

---

## Configuration JSON Assets

All CLI settings and commands are tracked in two local JSON profiles:

*   **`assets/commands.json`:** Mapped entries and command usage parameters, baked directly into the Rust binary at compile time.
*   **`Independent.json` / `system_control.json`:** Tracks active local configuration, hardware profiles, user identities, and loaded weight paths.
