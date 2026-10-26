# Models & Engine Vault

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`models.download`](./models/models.download.md) | write | task | Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first |
| [`models.inspect`](./models/models.inspect.md) | read | sync | Raw GGUF metadata and tensor table of an installed model |
| [`models.list`](./models/models.list.md) | read | sync | Installed registry entries, optionally filtered by category |
| [`models.load`](./models/models.load.md) | write | sync | Loads an installed GGUF model using the existing Engine host |
| [`models.remove`](./models/models.remove.md) | write | sync | Unloads the selected model if active and removes its installed package |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/models/`.
