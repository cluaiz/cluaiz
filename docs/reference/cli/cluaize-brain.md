# `cluaize brain` Command Reference

The `brain` command controls the C-FFI connection to the `cluaizd` background database daemon.

---

## 📋 Syntax & Usage

```bash
cluaize brain on [address]
cluaize brain off
cluaize brain only
cluaize brain status
```

### Examples:
```bash
cluaize brain on
cluaize brain on 10.0.0.5:8080
cluaize brain off
cluaize brain only
cluaize brain status
```

---

## ⚙️ Execution Flow & Modes

* **`on`:** Connects the local LLM orchestrator to a local or remote `cluaizd` database instance for memory retrieval.
* **`off`:** Suspends database synchronization.
* **`only`:** Activates local DB structures but unloads/suspends the LLM from VRAM to optimize system resource profiles.
* **`status`:** Validates connection packets and displays active latency.
