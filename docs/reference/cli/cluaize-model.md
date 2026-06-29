# `cluaiz model` Command Reference

The `model` command configures default settings inside `Permission.json`.

---

## 📋 Syntax & Usage

```bash
cluaiz model set-chat <model-identifier>
cluaiz model set-vector <model-identifier>
```

### Examples:
```bash
cluaiz model set-chat gemma4:e2b
cluaiz model set-vector bge_m3:unknown:onnx:fp32
```

---

## ⚙️ Configuration Targets

* **`set-chat`:** Sets the active language model key used when running standard chat loops.
* **`set-vector`:** Designates the embedding pipeline model used for document vectorization.
