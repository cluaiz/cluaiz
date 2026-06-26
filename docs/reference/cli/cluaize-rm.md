# `cluaize rm` Command Reference

The `rm` command deletes a downloaded model binary from the local workstation.

---

## 📋 Syntax & Usage

```bash
cluaize rm <model-identifier>
```

---

## ⚙️ Execution Flow

1. **Active Check:** Verifies if the target model is currently loaded in memory. If loaded, sends a hotswap command to unload it first.
2. **De-Registration:** Removes metadata records from indices.
3. **File Purge:** Unlinks the GGUF file from `~/.cluaize/models/`.
