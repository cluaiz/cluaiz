# `cluaize skill` Command Reference

The `skill` command manages sandboxed WASM extensions/plugins.

---

## 📋 Syntax & Usage

```bash
cluaize skill install <name>
cluaize skill list
cluaize skill cache ls
cluaize skill cache clear [options]
```

### Options:
* `cache clear --all`: Triggers cleanup of all orphaned caches.
* `cache clear <model-id> --force`: Forcefully unlinks cache files for a specific model key.

### Examples:
```bash
cluaize skill install web-search-github
cluaize skill list
cluaize skill cache ls
```
