# `cluaiz permission` Command Reference

The `permission` command modifies security, vectorization, and firewall modes.

---

## 📋 Syntax & Usage

```bash
cluaiz permission [options]
```

### Options:
* `firewall <status>`: Choose WASM sandbox firewall level (`auto`, `strict`, `off`).
* `telemetry <status>`: Toggle anonymous telemetry logs (`on`, `off`).

### Examples:
```bash
cluaiz permission
cluaiz permission firewall strict
cluaiz permission telemetry off
```
