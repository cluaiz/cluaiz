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

## Files

* **`~/.cluaiz/config/settings.json`:** engine settings (`cluaiz config`).
* **`~/.cluaiz/config/permission.json`:** port, auth, security mode (`cluaiz permission`). See [permission.json](../engine/permission.md).
* **`~/.cluaiz/config/hardware_profile.json`:** the saved hardware probe (`cluaiz calibrate`). See [hardware profile](../engine/system_control.md).
* **`~/.cluaiz/config/model_registry.json`:** installed models.
