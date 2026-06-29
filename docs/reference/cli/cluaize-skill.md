# `cluaiz skill` Command Reference

The `skill` command manages sandboxed WASM extensions/plugins.

---

## 📋 Syntax & Usage

```bash
cluaiz skill install <name>
cluaiz skill list
cluaiz skill cache ls
cluaiz skill cache clear [options]
```

### Options:
* `cache clear --all`: Triggers cleanup of all orphaned caches.
* `cache clear <model-id> --force`: Forcefully unlinks cache files for a specific model key.

### Examples:
```bash
cluaiz skill install web-search-github
cluaiz skill list
cluaiz skill cache ls
```
