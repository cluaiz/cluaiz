# Models & Engine Vault

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`models.discard`](./models/models.discard.md) | write | sync | Deletes what a stopped models.download left (its `.part` and finished files), so the next download starts over; an installed model is never touched |
| [`models.download`](./models/models.download.md) | write | task | Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first |
| [`models.embedding_room`](./models/models.embedding_room.md) | read | sync | Where each embedding model would run beside the chat model (GPU, CPU or no room) on today's free memory, and the one to recommend |
| [`models.inspect`](./models/models.inspect.md) | read | sync | Raw GGUF metadata and tensor table of an installed model |
| [`models.list`](./models/models.list.md) | read | sync | Installed registry entries, optionally filtered by category |
| [`models.load`](./models/models.load.md) | write | sync | Loads an installed GGUF model using the existing Engine host |
| [`models.remove`](./models/models.remove.md) | write | sync | Unloads the selected model if active and removes its installed package |
| [`providers.list`](./providers/providers.list.md) | admin | sync | The model providers and their listed models; keys are never returned |
| [`providers.models`](./providers/providers.models.md) | admin | sync | The models a provider offers (its GET /models), with the context size it reports |
| [`providers.remove`](./providers/providers.remove.md) | admin | sync | Removes a provider, with its key if it was kept in the file |
| [`providers.set`](./providers/providers.set.md) | admin | sync | Adds a provider, or changes its URL, key source and listed models |
| [`providers.test`](./providers/providers.test.md) | admin | sync | Sends one short request of the model's kind: the URL, the key and the model work |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/models/`.
