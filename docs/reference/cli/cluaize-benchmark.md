# `cluaiz benchmark` Command Reference

The `benchmark` command runs standardized execution loops on model layers.

---

## 📋 Syntax & Usage

```bash
cluaiz benchmark [model-identifier] [options]
```

### Options:
* `--runs <N>`: Sets test generation iterations (default: `1`).

### Examples:
```bash
cluaiz benchmark
cluaiz benchmark gemma4:e2b --runs 3
```
