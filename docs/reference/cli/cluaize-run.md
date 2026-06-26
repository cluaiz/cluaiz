# `cluaize run` Command Reference

The `run` command loads and runs inference using a designated Large Language Model.

---

## 📋 Syntax & Usage

```bash
cluaize run <model-identifier> [options]
```

### Options:
* `--interactive <true/false>`: Toggle interactive console/TUI mode. If `false`, executes single-pass inference (default: `true`).
* `--runs <N>`: Sets generation iterations (used in batch testing).

### Examples:
```bash
cluaize run bonsai:8b
cluaize run gemma4:e2b --interactive false
```

---

## ⚙️ Execution Flow

1. **Vault Verification:** Verifies if the requested `<model-identifier>` is present in the local vault (`~/.cluaize/models/chat/`).
2. **Auto-Pull:** If the model is not found, dynamically initializes a download routine using HuggingFace registry URLs.
3. **Applies Booster Limits:** Reads `system_booster.json` to load the exact offload layer count and KV cache limitations before spawning the inference engine thread.
4. **Interface Launch:** Fires up the Ratatui TUI dashboard for interactive session management.
