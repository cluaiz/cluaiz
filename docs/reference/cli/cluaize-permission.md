# `cluaize permission` Command Reference

The `permission` command modifies security, vectorization, and firewall modes.

---

## 📋 Syntax & Usage

```bash
cluaize permission [options]
```

### Options:
* `firewall <status>`: Choose WASM sandbox firewall level (`auto`, `strict`, `off`).
* `telemetry <status>`: Toggle anonymous telemetry logs (`on`, `off`).

### Examples:
```bash
cluaize permission
cluaize permission firewall strict
cluaize permission telemetry off
```
